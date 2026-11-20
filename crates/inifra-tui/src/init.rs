use crate::{
    CatalogView, GenerationReview, InitBackend, InitOutcome, InputPrompt, SelectionPreview, view,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

const ERROR_DISPLAY_DURATION: Duration = Duration::from_secs(2);

/// Run the inline initialization flow in the current terminal.
///
/// The backend prepares a plan before review and applies it only after explicit
/// approval. Esc or Ctrl-C cancels without applying. A terminal is required.
///
/// # Errors
/// Returns an error if the terminal cannot be initialized, drawn, or read.
pub fn run_init<B: InitBackend>(catalog: CatalogView, backend: B) -> io::Result<InitOutcome> {
    run_init_with_selection(catalog, backend, &[], BTreeMap::new())
}

/// Runs initialization with a preset selection and saved values preloaded.
///
/// The user enters the ordinary selection and review flow with the supplied templates selected.
///
/// # Errors
/// Returns an error if a template is unknown, multiple templates map to the same exclusive
/// choice, or the terminal cannot be initialized, drawn, or read.
pub fn run_init_with_selection<B: InitBackend>(
    catalog: CatalogView,
    mut backend: B,
    selected_ids: &[String],
    values: BTreeMap<String, String>,
) -> io::Result<InitOutcome> {
    let mut app = App::new(catalog);
    app.select_initial(selected_ids)?;
    app.values = values;
    app.refresh_selection_preview(&mut backend)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    run_app(&mut app, &mut backend)
}

fn run_app<B, P>(app: &mut App<P>, backend: &mut B) -> io::Result<InitOutcome>
where
    B: InitBackend<Plan = P>,
{
    let mut terminal = super::terminal::InlineTerminal::new(screen_height(&app.screen)?)?;
    loop {
        app.expire_error(Instant::now());
        terminal.draw(|frame| view::draw(frame, app))?;
        let event = if let Some(error) = &app.error {
            let timeout = error.expires_at.saturating_duration_since(Instant::now());
            if !event::poll(timeout)? {
                app.expire_error(Instant::now());
                continue;
            }
            event::read()?
        } else {
            event::read()?
        };
        match event {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = app.handle(key, backend) {
                    if let Err(error) = terminal.close() {
                        if matches!(&outcome, InitOutcome::Applied(_)) {
                            eprintln!(
                                "Could not clear the terminal after applying changes: {error}"
                            );
                        } else {
                            return Err(error);
                        }
                    }
                    return Ok(outcome);
                }
                let height = screen_height(&app.screen)?;
                if terminal.height() != height {
                    terminal.close()?;
                    terminal = super::terminal::InlineTerminal::new(height)?;
                }
            }
            _ => {}
        }
    }
}

fn screen_height<P>(screen: &Screen<P>) -> io::Result<u16> {
    match screen {
        Screen::Select => super::terminal::inline_picker_height(),
        Screen::Input { .. } => super::terminal::inline_height(5, 5),
        Screen::Review { .. } => super::terminal::inline_height(20, 7),
    }
}

pub(crate) enum Screen<P> {
    Select,
    Input {
        prompts: Vec<InputPrompt>,
        index: usize,
    },
    Review {
        review: GenerationReview,
        plan: P,
        approve: bool,
    },
}

pub(crate) struct App<P> {
    pub catalog: CatalogView,
    pub selected: Vec<bool>,
    pub variants: Vec<usize>,
    pub resolved_outputs: BTreeMap<String, BTreeMap<PathBuf, PathBuf>>,
    pub unavailable_templates: BTreeSet<String>,
    pub cursor: usize,
    pub preview_scroll: u16,
    pub search_active: bool,
    pub search_query: String,
    pub screen: Screen<P>,
    pub values: BTreeMap<String, String>,
    pub input: String,
    pub input_cursor: usize,
    error: Option<TransientError>,
}

struct TransientError {
    message: String,
    expires_at: Instant,
}

impl<P> App<P> {
    pub fn new(catalog: CatalogView) -> Self {
        Self {
            selected: vec![false; catalog.templates.len()],
            variants: vec![0; catalog.templates.len()],
            resolved_outputs: BTreeMap::new(),
            unavailable_templates: BTreeSet::new(),
            catalog,
            cursor: 0,
            preview_scroll: 0,
            search_active: false,
            search_query: String::new(),
            screen: Screen::Select,
            values: BTreeMap::new(),
            input: String::new(),
            input_cursor: 0,
            error: None,
        }
    }

    pub(crate) fn error_message(&self) -> Option<&str> {
        self.error.as_ref().map(|error| error.message.as_str())
    }

    fn show_error(&mut self, message: impl Into<String>) {
        self.error = Some(TransientError {
            message: message.into(),
            expires_at: Instant::now() + ERROR_DISPLAY_DURATION,
        });
    }

    fn clear_error(&mut self) {
        self.error = None;
    }

    pub(super) fn visible_indices(&self) -> Vec<usize> {
        let query = self.search_query.to_lowercase();
        self.catalog
            .templates
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                if query.is_empty() {
                    return Some(index);
                }
                let matches = |value: &str| value.to_lowercase().contains(&query);
                let category_matches = self
                    .catalog
                    .categories
                    .iter()
                    .find(|category| category.id == choice.category_id)
                    .is_some_and(|category| matches(&category.name))
                    || matches(&choice.category_id);
                (category_matches
                    || choice.variants.iter().any(|template| {
                        matches(&template.id)
                            || matches(&template.description)
                            || template
                                .inputs
                                .iter()
                                .any(|input| matches(&input.name) || matches(&input.prompt))
                            || template.sources.iter().any(|source| {
                                matches(&source.source.to_string_lossy())
                                    || matches(&source.output.to_string_lossy())
                                    || source
                                        .alternate_output
                                        .as_ref()
                                        .is_some_and(|output| matches(&output.to_string_lossy()))
                            })
                            || template
                                .directories
                                .iter()
                                .any(|path| matches(&path.to_string_lossy()))
                    }))
                .then_some(index)
            })
            .collect()
    }

    fn search_changed(&mut self) {
        let visible = self.visible_indices();
        if !visible.contains(&self.cursor)
            && let Some(first) = visible.first()
        {
            self.cursor = *first;
        }
        self.selection_changed();
    }

    fn clear_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.selection_changed();
    }

    fn expire_error(&mut self, now: Instant) -> bool {
        if self
            .error
            .as_ref()
            .is_some_and(|error| error.expires_at <= now)
        {
            self.clear_error();
            true
        } else {
            false
        }
    }

    fn select_initial(&mut self, selected_ids: &[String]) -> io::Result<()> {
        for id in selected_ids {
            let found = self
                .catalog
                .templates
                .iter()
                .enumerate()
                .find_map(|(choice_index, choice)| {
                    choice
                        .variants
                        .iter()
                        .position(|template| template.id == *id)
                        .map(|variant_index| (choice_index, variant_index))
                })
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("template {id:?} cannot be selected in the initialization picker"),
                    )
                })?;
            let (choice_index, variant_index) = found;
            if self.selected[choice_index] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "the preset selects multiple alternatives in one shared-output group, including {id:?}"
                    ),
                ));
            }
            self.selected[choice_index] = true;
            self.variants[choice_index] = variant_index;
        }
        Ok(())
    }

    fn selected_ids(&self) -> Vec<String> {
        self.catalog
            .templates
            .iter()
            .zip(self.selected.iter().zip(&self.variants))
            .filter(|(_, (selected, _))| **selected)
            .filter_map(|(choice, (_, variant))| choice.variants.get(*variant))
            .map(|template| template.id.clone())
            .collect()
    }

    fn refresh_selection_preview<B: InitBackend<Plan = P>>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), String> {
        let preview = self.selection_preview_for(backend, &self.selected, &self.variants)?;
        self.apply_selection_preview(preview);
        Ok(())
    }

    fn selection_preview_for<B: InitBackend<Plan = P>>(
        &self,
        backend: &mut B,
        selected: &[bool],
        variants: &[usize],
    ) -> Result<SelectionPreview, String> {
        let mut preview = backend.selection_preview(&self.selected_ids_for(selected, variants))?;
        for (choice_index, choice) in self.catalog.templates.iter().enumerate() {
            if !selected[choice_index] || choice.variants.len() < 2 {
                continue;
            }
            for (variant_index, template) in choice.variants.iter().enumerate() {
                if variant_index == variants[choice_index] {
                    continue;
                }
                let mut replacement = variants.to_vec();
                replacement[choice_index] = variant_index;
                let ids = self.selected_ids_for(selected, &replacement);
                if backend.validate_selection(&ids).is_err() {
                    preview.unavailable.insert(template.id.clone());
                } else {
                    preview.unavailable.remove(&template.id);
                }
            }
        }
        Ok(preview)
    }

    fn apply_selection_preview(&mut self, preview: SelectionPreview) {
        self.resolved_outputs = preview.outputs;
        self.unavailable_templates = preview.unavailable;
    }

    fn select_move(&mut self, forward: bool) {
        let visible = self.visible_indices();
        let Some(position) = visible.iter().position(|index| *index == self.cursor) else {
            return;
        };
        let len = visible.len();
        self.cursor = visible[if forward {
            (position + 1) % len
        } else {
            (position + len - 1) % len
        }];
        self.selection_changed();
    }

    fn category_move(&mut self, forward: bool) {
        let visible = self.visible_indices();
        let len = visible.len();
        if len < 2 {
            return;
        }
        let Some(current_position) = visible.iter().position(|index| *index == self.cursor) else {
            return;
        };
        let current = &self.catalog.templates[self.cursor].category_id;
        let next = (1..len)
            .map(|offset| {
                if forward {
                    visible[(current_position + offset) % len]
                } else {
                    visible[(current_position + len - offset) % len]
                }
            })
            .find(|&index| &self.catalog.templates[index].category_id != current);
        if let Some(index) = next {
            let category = &self.catalog.templates[index].category_id;
            self.cursor = visible
                .iter()
                .copied()
                .find(|index| &self.catalog.templates[*index].category_id == category)
                .unwrap_or(index);
            self.selection_changed();
        }
    }

    fn variant_move<B: InitBackend<Plan = P>>(&mut self, forward: bool, backend: &mut B) {
        let Some(choice) = self.catalog.templates.get(self.cursor) else {
            return;
        };
        if choice.variants.len() < 2 {
            return;
        }
        let count = choice.variants.len();
        let next_variant = if forward {
            (self.variants[self.cursor] + 1) % count
        } else {
            (self.variants[self.cursor] + count - 1) % count
        };
        if self.selected[self.cursor]
            && self
                .unavailable_templates
                .contains(&choice.variants[next_variant].id)
        {
            return;
        }
        let mut variants = self.variants.clone();
        variants[self.cursor] = next_variant;
        if self.selected[self.cursor] {
            match self.selection_preview_for(backend, &self.selected, &variants) {
                Ok(preview) => self.apply_selection_preview(preview),
                Err(error) => {
                    self.show_error(error);
                    return;
                }
            }
            self.values.clear();
        }
        self.variants[self.cursor] = next_variant;
        self.selection_changed();
    }

    fn selected_ids_for(&self, selected: &[bool], variants: &[usize]) -> Vec<String> {
        self.catalog
            .templates
            .iter()
            .zip(selected.iter().zip(variants))
            .filter(|(_, (selected, _))| **selected)
            .filter_map(|(choice, (_, variant))| choice.variants.get(*variant))
            .map(|template| template.id.clone())
            .collect()
    }

    fn toggle_current<B: InitBackend<Plan = P>>(&mut self, backend: &mut B) {
        let Some(selected) = self.selected.get(self.cursor).copied() else {
            return;
        };
        if !selected {
            let template_id = self.catalog.templates[self.cursor].variants
                [self.variants[self.cursor]]
                .id
                .clone();
            if self.unavailable_templates.contains(&template_id) {
                return;
            }
        }
        let mut candidate = self.selected.clone();
        candidate[self.cursor] = !selected;
        match self.selection_preview_for(backend, &candidate, &self.variants) {
            Ok(preview) => self.apply_selection_preview(preview),
            Err(error) => {
                self.show_error(error);
                return;
            }
        }
        self.selected = candidate;
        self.values.clear();
        self.selection_changed();
    }

    fn toggle_category<B: InitBackend<Plan = P>>(&mut self, backend: &mut B) {
        let Some(category_id) = self
            .catalog
            .templates
            .get(self.cursor)
            .map(|choice| choice.category_id.clone())
        else {
            return;
        };
        let indices = self
            .visible_indices()
            .into_iter()
            .filter(|index| self.catalog.templates[*index].category_id == category_id)
            .collect::<Vec<_>>();
        let select = indices.iter().any(|index| {
            if self.selected[*index] {
                return false;
            }
            let template_id = &self.catalog.templates[*index].variants[self.variants[*index]].id;
            !self.unavailable_templates.contains(template_id)
        });
        let mut candidate = self.selected.clone();
        if select {
            for index in &indices {
                if candidate[*index] {
                    continue;
                }
                let template_id = self.catalog.templates[*index].variants[self.variants[*index]]
                    .id
                    .clone();
                if self.unavailable_templates.contains(&template_id) {
                    continue;
                }
                candidate[*index] = true;
                if backend
                    .validate_selection(&self.selected_ids_for(&candidate, &self.variants))
                    .is_err()
                {
                    candidate[*index] = false;
                }
            }
        } else {
            for index in indices {
                candidate[index] = false;
            }
        }
        match self.selection_preview_for(backend, &candidate, &self.variants) {
            Ok(preview) => self.apply_selection_preview(preview),
            Err(error) => {
                self.show_error(error);
                return;
            }
        }
        self.selected = candidate;
        self.values.clear();
        self.selection_changed();
    }

    fn selection_changed(&mut self) {
        self.preview_scroll = 0;
        self.clear_error();
    }

    fn input_value(&mut self, value: String) {
        self.input_cursor = value.chars().count();
        self.input = value;
    }

    fn prepare<B: InitBackend<Plan = P>>(&mut self, backend: &mut B) {
        match backend.prepare(&self.selected_ids(), &self.values) {
            Ok((review, plan)) => {
                self.clear_error();
                self.preview_scroll = 0;
                self.screen = Screen::Review {
                    review,
                    plan,
                    approve: false,
                };
            }
            Err(error) => {
                self.screen = Screen::Select;
                self.show_error(error);
            }
        }
    }

    pub fn handle<B: InitBackend<Plan = P>>(
        &mut self,
        key: KeyEvent,
        backend: &mut B,
    ) -> Option<InitOutcome> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(InitOutcome::Cancelled);
        }
        let accepting_text = matches!(&self.screen, Screen::Input { .. })
            || (matches!(&self.screen, Screen::Select) && self.search_active);
        if key.code == KeyCode::Char('q') && key.modifiers == KeyModifiers::NONE && !accepting_text
        {
            return Some(InitOutcome::Cancelled);
        }
        if key.code == KeyCode::Esc {
            if matches!(&self.screen, Screen::Select)
                && (self.search_active || !self.search_query.is_empty())
            {
                self.clear_search();
                return None;
            }
            return Some(InitOutcome::Cancelled);
        }
        if matches!(&self.screen, Screen::Select) && self.search_active {
            match key.code {
                KeyCode::Enter => {
                    self.search_active = false;
                    self.clear_error();
                    return None;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.search_changed();
                    return None;
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.search_query.push(character);
                    self.search_changed();
                    return None;
                }
                _ => {}
            }
        }
        match &mut self.screen {
            Screen::Select => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.select_move(true),
                KeyCode::Up | KeyCode::Char('k') => self.select_move(false),
                KeyCode::Right | KeyCode::Char('l') => self.variant_move(true, backend),
                KeyCode::Left | KeyCode::Char('h') => self.variant_move(false, backend),
                KeyCode::Tab => self.category_move(true),
                KeyCode::BackTab => self.category_move(false),
                KeyCode::PageDown => self.preview_scroll = self.preview_scroll.saturating_add(8),
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(8),
                KeyCode::Char(' ') => self.toggle_current(backend),
                KeyCode::Char('a') => self.toggle_category(backend),
                KeyCode::Char('/') => {
                    self.search_active = true;
                    self.clear_error();
                }
                KeyCode::Enter => {
                    let selected = self.selected_ids();
                    match backend.validate_selection(&selected) {
                        Err(error) => self.show_error(error),
                        Ok(()) => match backend.input_prompts(&selected) {
                            Ok(prompts) if prompts.is_empty() => self.prepare(backend),
                            Ok(mut prompts) => {
                                prompts.sort_by_key(|prompt| !prompt.required);
                                if let Some(prompt) = prompts.first() {
                                    let value = self
                                        .values
                                        .get(&prompt.name)
                                        .cloned()
                                        .or_else(|| prompt.default.clone())
                                        .unwrap_or_default();
                                    self.input_value(value);
                                }
                                self.clear_error();
                                self.screen = Screen::Input { prompts, index: 0 };
                            }
                            Err(error) => self.show_error(error),
                        },
                    }
                }
                _ => {}
            },
            Screen::Input { prompts, index } => match key.code {
                KeyCode::Left => self.input_cursor = self.input_cursor.saturating_sub(1),
                KeyCode::Right => {
                    self.input_cursor = (self.input_cursor + 1).min(self.input.chars().count())
                }
                KeyCode::Home => self.input_cursor = 0,
                KeyCode::End => self.input_cursor = self.input.chars().count(),
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    let byte_index = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor)
                        .map_or(self.input.len(), |(index, _)| index);
                    self.input.insert(byte_index, ch);
                    self.input_cursor += 1;
                    self.error = None;
                }
                KeyCode::Backspace if self.input_cursor > 0 => {
                    let start = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor - 1)
                        .map_or(self.input.len(), |(index, _)| index);
                    let end = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor)
                        .map_or(self.input.len(), |(index, _)| index);
                    self.input.replace_range(start..end, "");
                    self.input_cursor -= 1;
                    self.error = None;
                }
                KeyCode::Delete if self.input_cursor < self.input.chars().count() => {
                    let start = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor)
                        .map_or(self.input.len(), |(index, _)| index);
                    let end = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor + 1)
                        .map_or(self.input.len(), |(index, _)| index);
                    self.input.replace_range(start..end, "");
                    self.error = None;
                }
                KeyCode::Enter => {
                    let prompt = &prompts[*index];
                    let supplied = if self.input.is_empty() {
                        prompt.default.as_deref().unwrap_or("")
                    } else {
                        &self.input
                    };
                    if prompt.required && supplied.trim().is_empty() {
                        self.show_error("A value is required.");
                    } else {
                        self.values.insert(prompt.name.clone(), supplied.to_owned());
                        self.error = None;
                        *index += 1;
                        if *index == prompts.len() {
                            self.prepare(backend);
                        } else {
                            let value = self
                                .values
                                .get(&prompts[*index].name)
                                .cloned()
                                .or_else(|| prompts[*index].default.clone())
                                .unwrap_or_default();
                            self.input_value(value);
                        }
                    }
                }
                _ => {}
            },
            Screen::Review { approve, .. } => match key.code {
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('h' | 'j' | 'k' | 'l')
                | KeyCode::Char(' ') => *approve = !*approve,
                KeyCode::PageDown => self.preview_scroll = self.preview_scroll.saturating_add(8),
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(8),
                KeyCode::Enter if !*approve => return Some(InitOutcome::Cancelled),
                KeyCode::Enter => {
                    let Screen::Review { review, plan, .. } =
                        std::mem::replace(&mut self.screen, Screen::Select)
                    else {
                        unreachable!()
                    };
                    match backend.apply(plan) {
                        Ok(()) => return Some(InitOutcome::Applied(review)),
                        Err(error) => self.show_error(error),
                    }
                }
                _ => {}
            },
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CategoryView, GenerationReview, TemplateChoice, TemplateInputView, TemplateSourceView,
        TemplateView,
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[derive(Default)]
    struct FakeBackend {
        applied: Vec<u32>,
        prepared: Vec<Vec<String>>,
        require_input: bool,
        fail_prepare: bool,
        conflicting_ids: Option<(String, String)>,
    }

    impl InitBackend for FakeBackend {
        type Plan = u32;
        fn input_prompts(&mut self, _: &[String]) -> Result<Vec<InputPrompt>, String> {
            Ok(if self.require_input {
                vec![InputPrompt {
                    name: "name".into(),
                    prompt: "Project name".into(),
                    required: true,
                    default: None,
                }]
            } else {
                Vec::new()
            })
        }
        fn validate_selection(&mut self, selected: &[String]) -> Result<(), String> {
            if selected.is_empty() {
                Err("select at least one template".into())
            } else if let Some((left, right)) = &self.conflicting_ids
                && selected.contains(left)
                && selected.contains(right)
            {
                Err("duplicate output AGENTS.md".into())
            } else {
                Ok(())
            }
        }
        fn selection_preview(&mut self, selected: &[String]) -> Result<SelectionPreview, String> {
            let mut preview = SelectionPreview::default();
            if let Some((left, right)) = &self.conflicting_ids {
                if selected.contains(left) {
                    preview.unavailable.insert(right.clone());
                }
                if selected.contains(right) {
                    preview.unavailable.insert(left.clone());
                }
            }
            Ok(preview)
        }
        fn prepare(
            &mut self,
            selected: &[String],
            _: &BTreeMap<String, String>,
        ) -> Result<(GenerationReview, u32), String> {
            if self.fail_prepare {
                return Err("Invalid plan".into());
            }
            self.prepared.push(selected.to_vec());
            Ok((
                GenerationReview {
                    target: ".".into(),
                    files: Vec::new(),
                    directories: Vec::new(),
                },
                42,
            ))
        }
        fn apply(&mut self, plan: u32) -> Result<(), String> {
            self.applied.push(plan);
            Ok(())
        }
    }

    fn template(id: &str) -> TemplateView {
        TemplateView {
            id: id.into(),
            description: format!("Description for {id}"),
            inputs: vec![TemplateInputView {
                name: "project_name".into(),
                prompt: "Project name".into(),
            }],
            directories: Vec::new(),
            sources: vec![TemplateSourceView {
                source: "README.md".into(),
                output: "README.md".into(),
                alternate_output: None,
                content: format!("# Source preview for {id}"),
            }],
        }
    }

    fn app() -> App<u32> {
        App::new(CatalogView {
            categories: vec![
                CategoryView {
                    id: "one".into(),
                    name: "One".into(),
                },
                CategoryView {
                    id: "two".into(),
                    name: "Two".into(),
                },
            ],
            templates: vec![
                TemplateChoice {
                    category_id: "one".into(),
                    variants: vec![template("template-0"), template("template-0-alt")],
                },
                TemplateChoice {
                    category_id: "one".into(),
                    variants: vec![template("template-1")],
                },
                TemplateChoice {
                    category_id: "two".into(),
                    variants: vec![template("template-2")],
                },
            ],
        })
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn navigation_wraps_and_category_keys_select_the_next_category() {
        let mut app = app();
        let mut backend = FakeBackend::default();
        app.handle(key(KeyCode::Up), &mut backend);
        assert_eq!(app.cursor, 2);
        app.handle(key(KeyCode::Down), &mut backend);
        assert_eq!(app.cursor, 0);
        app.handle(key(KeyCode::Tab), &mut backend);
        assert_eq!(app.cursor, 2);
        app.handle(key(KeyCode::BackTab), &mut backend);
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn preset_selection_preselects_the_matching_shared_output_variant() {
        let mut app = app();

        app.select_initial(&["template-0-alt".into(), "template-2".into()])
            .unwrap();

        assert_eq!(app.selected, [true, false, true]);
        assert_eq!(app.variants, [1, 0, 0]);
        assert_eq!(app.selected_ids(), ["template-0-alt", "template-2"]);
    }

    #[test]
    fn enter_without_a_selection_keeps_the_picker_open_and_shows_an_error() {
        let mut app = app();
        let mut backend = FakeBackend::default();

        let outcome = app.handle(key(KeyCode::Enter), &mut backend);

        assert!(outcome.is_none());
        assert!(matches!(app.screen, Screen::Select));
        assert_eq!(app.error_message(), Some("select at least one template"));
    }

    #[test]
    fn transient_errors_expire_and_restore_the_keybind_footer() {
        let mut app = app();
        app.show_error("select at least one template");
        assert_eq!(app.error_message(), Some("select at least one template"));

        let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        let error_footer = format!("{:?}", terminal.backend().buffer());
        assert!(error_footer.contains("select at least one template"));

        assert!(app.expire_error(Instant::now() + ERROR_DISPLAY_DURATION));

        assert_eq!(app.error_message(), None);
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        let keybind_footer = format!("{:?}", terminal.backend().buffer());
        assert!(!keybind_footer.contains("select at least one template"));
        assert!(keybind_footer.contains("Space select"));
    }

    #[test]
    fn picker_prevents_a_conflict_and_keeps_the_keybind_footer() {
        let mut app = app();
        let mut backend = FakeBackend {
            conflicting_ids: Some(("template-0".into(), "template-1".into())),
            ..FakeBackend::default()
        };
        app.refresh_selection_preview(&mut backend).unwrap();
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Down), &mut backend);

        app.handle(key(KeyCode::Char(' ')), &mut backend);

        assert_eq!(app.selected, [true, false, false]);
        assert!(app.unavailable_templates.contains("template-1"));
        assert_eq!(app.error_message(), None);
        let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("Space select"));
        assert!(!text.contains("duplicate output"));
    }

    #[test]
    fn group_toggle_clears_available_items_when_conflicts_block_the_rest() {
        let mut app = app();
        let mut backend = FakeBackend {
            conflicting_ids: Some(("template-0".into(), "template-1".into())),
            ..FakeBackend::default()
        };

        app.handle(key(KeyCode::Char('a')), &mut backend);

        assert_eq!(app.selected, [true, false, false]);
        assert!(app.unavailable_templates.contains("template-1"));

        app.handle(key(KeyCode::Char('a')), &mut backend);

        assert_eq!(app.selected, [false, false, false]);
        assert!(app.unavailable_templates.is_empty());
    }

    #[test]
    fn slash_search_filters_live_and_keeps_a_available_for_group_toggle() {
        let mut app = app();
        let mut backend = FakeBackend::default();

        app.handle(key(KeyCode::Char('/')), &mut backend);
        assert!(app.search_active);

        for character in "template-2".chars() {
            app.handle(key(KeyCode::Char(character)), &mut backend);
        }
        assert_eq!(app.search_query, "template-2");
        assert_eq!(app.visible_indices(), [2]);
        assert_eq!(app.cursor, 2);

        app.handle(key(KeyCode::Char('a')), &mut backend);
        assert_eq!(app.search_query, "template-2a");
        assert!(app.selected.iter().all(|selected| !selected));

        app.handle(key(KeyCode::Backspace), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(!app.search_active);
        assert_eq!(app.search_query, "template-2");

        app.handle(key(KeyCode::Char('a')), &mut backend);
        assert!(app.selected[2]);

        app.handle(key(KeyCode::Char('/')), &mut backend);
        app.handle(key(KeyCode::Esc), &mut backend);
        assert!(!app.search_active);
        assert!(app.search_query.is_empty());
        assert_eq!(app.visible_indices(), [0, 1, 2]);
    }

    #[test]
    fn escape_clears_an_applied_filter_before_cancelling_the_picker() {
        let mut app = app();
        let mut backend = FakeBackend::default();

        app.handle(key(KeyCode::Char('/')), &mut backend);
        for character in "template-2".chars() {
            app.handle(key(KeyCode::Char(character)), &mut backend);
        }
        app.handle(key(KeyCode::Enter), &mut backend);

        assert!(!app.search_active);
        assert_eq!(app.search_query, "template-2");

        assert!(app.handle(key(KeyCode::Esc), &mut backend).is_none());
        assert!(app.search_query.is_empty());
        assert!(matches!(app.screen, Screen::Select));
        assert_eq!(app.visible_indices(), [0, 1, 2]);

        assert!(matches!(
            app.handle(key(KeyCode::Esc), &mut backend),
            Some(InitOutcome::Cancelled)
        ));
    }

    #[test]
    fn q_cancels_without_text_input_but_remains_search_and_input_text() {
        let mut picker = app();
        let mut backend = FakeBackend::default();

        assert!(matches!(
            picker.handle(key(KeyCode::Char('q')), &mut backend),
            Some(InitOutcome::Cancelled)
        ));

        let mut review = app();
        review.screen = Screen::Review {
            review: GenerationReview {
                target: ".".into(),
                files: Vec::new(),
                directories: Vec::new(),
            },
            plan: 42,
            approve: false,
        };
        assert!(matches!(
            review.handle(key(KeyCode::Char('q')), &mut backend),
            Some(InitOutcome::Cancelled)
        ));

        let mut searching = app();
        searching.handle(key(KeyCode::Char('/')), &mut backend);
        assert!(
            searching
                .handle(key(KeyCode::Char('q')), &mut backend)
                .is_none()
        );
        assert_eq!(searching.search_query, "q");

        let mut input = app();
        input.screen = Screen::Input {
            prompts: vec![InputPrompt {
                name: "name".into(),
                prompt: "Project name".into(),
                required: false,
                default: None,
            }],
            index: 0,
        };
        assert!(
            input
                .handle(key(KeyCode::Char('q')), &mut backend)
                .is_none()
        );
        assert_eq!(input.input, "q");
    }

    #[test]
    fn preset_selection_rejects_multiple_exclusive_variants() {
        let mut app = app();

        let error = app
            .select_initial(&["template-0".into(), "template-0-alt".into()])
            .unwrap_err();

        assert!(error.to_string().contains("multiple alternatives"));
    }

    #[test]
    fn cycling_shared_output_variants_keeps_one_template_selected() {
        let mut app = app();
        let mut backend = FakeBackend::default();
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Right), &mut backend);
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("Source preview for template-0-alt"));
        app.handle(key(KeyCode::Enter), &mut backend);
        assert_eq!(backend.prepared, vec![vec!["template-0-alt"]]);
        assert!(matches!(app.screen, Screen::Review { .. }));
    }

    #[test]
    fn cancelling_and_default_no_do_not_apply_the_plan() {
        let mut app = app();
        let mut backend = FakeBackend::default();
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(matches!(app.screen, Screen::Review { approve: false, .. }));
        assert!(matches!(
            app.handle(key(KeyCode::Enter), &mut backend),
            Some(InitOutcome::Cancelled)
        ));
        assert!(backend.applied.is_empty());
    }

    #[test]
    fn approval_applies_exactly_the_prepared_plan() {
        let mut app = app();
        let mut backend = FakeBackend::default();
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        app.handle(key(KeyCode::Char('l')), &mut backend);
        assert!(matches!(
            app.handle(key(KeyCode::Enter), &mut backend),
            Some(InitOutcome::Applied(_))
        ));
        assert_eq!(backend.applied, vec![42]);
    }

    #[test]
    fn missing_required_input_and_prepare_errors_remain_recoverable() {
        let mut app = app();
        let mut backend = FakeBackend {
            require_input: true,
            fail_prepare: true,
            ..FakeBackend::default()
        };
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(matches!(app.screen, Screen::Input { index: 0, .. }));
        assert_eq!(app.error_message(), Some("A value is required."));
        app.handle(key(KeyCode::Char('a')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(matches!(app.screen, Screen::Select));
        assert_eq!(app.error_message(), Some("Invalid plan"));
        assert!(backend.applied.is_empty());
    }

    #[test]
    fn text_input_keeps_h_and_l_as_text_and_arrows_edit_the_cursor() {
        let mut app = app();
        app.screen = Screen::Input {
            prompts: vec![InputPrompt {
                name: "name".into(),
                prompt: "Name".into(),
                required: false,
                default: None,
            }],
            index: 0,
        };
        let mut backend = FakeBackend::default();

        app.handle(key(KeyCode::Char('h')), &mut backend);
        app.handle(key(KeyCode::Char('l')), &mut backend);
        app.handle(key(KeyCode::Left), &mut backend);
        app.handle(key(KeyCode::Char('i')), &mut backend);

        assert_eq!(app.input, "hil");
        assert_eq!(app.input_cursor, 2);
    }

    #[test]
    fn input_cancellation_leaves_the_plan_unapplied() {
        let mut app = app();
        let mut backend = FakeBackend {
            require_input: true,
            ..FakeBackend::default()
        };
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(matches!(app.screen, Screen::Input { index: 0, .. }));

        assert!(matches!(
            app.handle(key(KeyCode::Esc), &mut backend),
            Some(InitOutcome::Cancelled)
        ));
        assert!(backend.applied.is_empty());
    }

    #[test]
    fn picker_renders_on_wide_and_narrow_terminals() {
        for width in [45, 100] {
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
            terminal.draw(|frame| view::draw(frame, &app())).unwrap();
            let text = format!("{:?}", terminal.backend().buffer());
            assert!(text.contains("template-0"));
            assert!(text.contains("Description"));
            assert!(text.contains("README.md"));
        }
    }
}
