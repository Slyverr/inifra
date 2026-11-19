use crate::{CatalogView, GenerationReview, InitBackend, InitOutcome, InputPrompt, view};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{TerminalOptions, Viewport};
use std::{collections::BTreeMap, io};

const VIEWPORT_HEIGHT: u16 = 24;

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::cursor::Show);
    }
}

/// Run the inline initialization flow in the current terminal.
///
/// The backend prepares a plan before review and applies it only after explicit
/// approval. Esc or Ctrl-C cancels without applying. A terminal is required.
///
/// # Errors
/// Returns an error if the terminal cannot be initialized, drawn, or read.
pub fn run_init<B: InitBackend>(catalog: CatalogView, mut backend: B) -> io::Result<InitOutcome> {
    let _restore = RestoreTerminal;
    let mut terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(VIEWPORT_HEIGHT),
    })?;
    let mut app = App::new(catalog);
    loop {
        terminal.draw(|frame| view::draw(frame, &app))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = app.handle(key, &mut backend) {
                    if let Err(error) = terminal.clear() {
                        if outcome == InitOutcome::Applied {
                            eprintln!(
                                "Could not clear the terminal after applying changes: {error}"
                            );
                        } else {
                            return Err(error);
                        }
                    }
                    return Ok(outcome);
                }
            }
            _ => {}
        }
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
    pub cursor: usize,
    pub preview_scroll: u16,
    pub screen: Screen<P>,
    pub values: BTreeMap<String, String>,
    pub input: String,
    pub error: Option<String>,
}

impl<P> App<P> {
    pub fn new(catalog: CatalogView) -> Self {
        Self {
            selected: vec![false; catalog.templates.len()],
            variants: vec![0; catalog.templates.len()],
            catalog,
            cursor: 0,
            preview_scroll: 0,
            screen: Screen::Select,
            values: BTreeMap::new(),
            input: String::new(),
            error: None,
        }
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

    fn select_move(&mut self, forward: bool) {
        let len = self.catalog.templates.len();
        if len > 0 {
            self.cursor = if forward {
                (self.cursor + 1) % len
            } else {
                (self.cursor + len - 1) % len
            };
            self.preview_scroll = 0;
        }
    }

    fn category_move(&mut self, forward: bool) {
        let len = self.catalog.templates.len();
        if len < 2 {
            return;
        }
        let current = &self.catalog.templates[self.cursor].category_id;
        let next = (1..len)
            .map(|offset| {
                if forward {
                    (self.cursor + offset) % len
                } else {
                    (self.cursor + len - offset) % len
                }
            })
            .find(|&index| &self.catalog.templates[index].category_id != current);
        if let Some(index) = next {
            let category = &self.catalog.templates[index].category_id;
            self.cursor = self
                .catalog
                .templates
                .iter()
                .position(|item| &item.category_id == category)
                .unwrap_or(index);
            self.preview_scroll = 0;
        }
    }

    fn variant_move(&mut self, forward: bool) {
        let Some(choice) = self.catalog.templates.get(self.cursor) else {
            return;
        };
        if choice.variants.len() < 2 {
            return;
        }
        let count = choice.variants.len();
        self.variants[self.cursor] = if forward {
            (self.variants[self.cursor] + 1) % count
        } else {
            (self.variants[self.cursor] + count - 1) % count
        };
        if self.selected[self.cursor] {
            self.values.clear();
        }
        self.preview_scroll = 0;
    }

    fn prepare<B: InitBackend<Plan = P>>(&mut self, backend: &mut B) {
        match backend.prepare(&self.selected_ids(), &self.values) {
            Ok((review, plan)) => {
                self.error = None;
                self.preview_scroll = 0;
                self.screen = Screen::Review {
                    review,
                    plan,
                    approve: false,
                };
            }
            Err(error) => {
                self.screen = Screen::Select;
                self.error = Some(error);
            }
        }
    }

    fn back_to_previous_input(&mut self) {
        let Screen::Input { prompts, index } = std::mem::replace(&mut self.screen, Screen::Select)
        else {
            return;
        };
        if index == 0 {
            self.values.clear();
        } else {
            let previous = index - 1;
            self.input = self
                .values
                .remove(&prompts[previous].name)
                .unwrap_or_default();
            self.screen = Screen::Input {
                prompts,
                index: previous,
            };
        }
        self.error = None;
    }

    pub fn handle<B: InitBackend<Plan = P>>(
        &mut self,
        key: KeyEvent,
        backend: &mut B,
    ) -> Option<InitOutcome> {
        if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Some(InitOutcome::Cancelled);
        }
        if matches!(self.screen, Screen::Input { .. }) && key.code == KeyCode::BackTab {
            self.back_to_previous_input();
            return None;
        }
        if matches!(self.screen, Screen::Review { .. })
            && matches!(key.code, KeyCode::Char('b') | KeyCode::Backspace)
        {
            self.screen = Screen::Select;
            self.preview_scroll = 0;
            self.error = None;
            return None;
        }
        match &mut self.screen {
            Screen::Select => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.select_move(true),
                KeyCode::Up | KeyCode::Char('k') => self.select_move(false),
                KeyCode::Right | KeyCode::Char('l') => self.variant_move(true),
                KeyCode::Left | KeyCode::Char('h') => self.variant_move(false),
                KeyCode::Tab => self.category_move(true),
                KeyCode::BackTab => self.category_move(false),
                KeyCode::PageDown => self.preview_scroll = self.preview_scroll.saturating_add(8),
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(8),
                KeyCode::Char(' ') => {
                    if let Some(selected) = self.selected.get_mut(self.cursor) {
                        *selected = !*selected;
                        self.values.clear();
                    }
                    self.error = None;
                }
                KeyCode::Enter => {
                    let selected = self.selected_ids();
                    if selected.is_empty() {
                        self.error = Some("Select at least one template.".into());
                    } else {
                        match backend.input_prompts(&selected) {
                            Ok(prompts) if prompts.is_empty() => {
                                self.values.clear();
                                self.prepare(backend);
                            }
                            Ok(mut prompts) => {
                                prompts.sort_by_key(|prompt| !prompt.required);
                                self.input = prompts
                                    .first()
                                    .and_then(|prompt| self.values.get(&prompt.name))
                                    .cloned()
                                    .unwrap_or_default();
                                self.error = None;
                                self.screen = Screen::Input { prompts, index: 0 };
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
                _ => {}
            },
            Screen::Input { prompts, index } => match key.code {
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.input.push(ch);
                    self.error = None;
                }
                KeyCode::Backspace => {
                    self.input.pop();
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
                        self.error = Some("A value is required.".into());
                    } else {
                        self.values.insert(prompt.name.clone(), supplied.to_owned());
                        self.error = None;
                        *index += 1;
                        if *index == prompts.len() {
                            self.prepare(backend);
                        } else {
                            self.input = self
                                .values
                                .get(&prompts[*index].name)
                                .cloned()
                                .unwrap_or_default();
                        }
                    }
                }
                _ => {}
            },
            Screen::Review { approve, .. } => match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Char('h' | 'l') | KeyCode::Tab => {
                    *approve = !*approve
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.preview_scroll = self.preview_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.preview_scroll = self.preview_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => self.preview_scroll = self.preview_scroll.saturating_add(8),
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(8),
                KeyCode::Char('y') => *approve = true,
                KeyCode::Char('n') => *approve = false,
                KeyCode::Enter if !*approve => return Some(InitOutcome::Cancelled),
                KeyCode::Enter => {
                    let Screen::Review { plan, .. } =
                        std::mem::replace(&mut self.screen, Screen::Select)
                    else {
                        unreachable!()
                    };
                    match backend.apply(plan) {
                        Ok(()) => return Some(InitOutcome::Applied),
                        Err(error) => self.error = Some(error),
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
    use crate::{CategoryView, GenerationReview, TemplateChoice, TemplateView};
    use ratatui::{Terminal, backend::TestBackend};

    #[derive(Default)]
    struct FakeBackend {
        applied: Vec<u32>,
        prepared: Vec<Vec<String>>,
        require_input: bool,
        fail_prepare: bool,
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
            outputs: vec!["README.md".into()],
            directories: Vec::new(),
            preview_source: Some("README.md.j2".into()),
            preview: format!("Source preview for {id}"),
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
        assert_eq!(
            app.handle(key(KeyCode::Enter), &mut backend),
            Some(InitOutcome::Cancelled)
        );
        assert!(backend.applied.is_empty());
    }

    #[test]
    fn approval_applies_exactly_the_prepared_plan() {
        let mut app = app();
        let mut backend = FakeBackend::default();
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        app.handle(key(KeyCode::Char('l')), &mut backend);
        assert_eq!(
            app.handle(key(KeyCode::Enter), &mut backend),
            Some(InitOutcome::Applied)
        );
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
        assert_eq!(app.error.as_deref(), Some("A value is required."));
        app.handle(key(KeyCode::Char('a')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(matches!(app.screen, Screen::Select));
        assert_eq!(app.error.as_deref(), Some("Invalid plan"));
        assert!(backend.applied.is_empty());
    }

    #[test]
    fn review_and_inputs_can_return_to_selection_without_applying() {
        let mut app = app();
        let mut backend = FakeBackend {
            require_input: true,
            ..FakeBackend::default()
        };
        app.handle(key(KeyCode::Char(' ')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        app.handle(key(KeyCode::Char('A')), &mut backend);
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(matches!(app.screen, Screen::Review { .. }));

        app.handle(key(KeyCode::Char('b')), &mut backend);
        assert!(matches!(app.screen, Screen::Select));
        app.handle(key(KeyCode::Enter), &mut backend);
        assert!(matches!(app.screen, Screen::Input { index: 0, .. }));
        assert_eq!(app.input, "A");

        app.handle(key(KeyCode::BackTab), &mut backend);
        assert!(matches!(app.screen, Screen::Select));
        assert!(app.values.is_empty());
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
            assert!(text.contains("README.md.j2"));
        }
    }
}
