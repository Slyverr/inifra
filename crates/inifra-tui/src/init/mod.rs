//! Interactive initialization state and screens.

mod events;
mod picker;
mod preview;
mod view;

use crate::{
    CatalogView, GenerationReview, InitBackend, InitOutcome, InputPrompt, SelectionPreview,
};
use crossterm::event::{self, Event, KeyEventKind};
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
}

#[cfg(test)]
mod tests;
