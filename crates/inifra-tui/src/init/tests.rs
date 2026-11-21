use super::*;
use crate::{
    CategoryView, GenerationReview, TemplateChoice, TemplateInputView, TemplateSourceView,
    TemplateView,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
