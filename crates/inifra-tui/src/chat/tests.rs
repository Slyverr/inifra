use super::*;
use crate::presentation::{Tone, tui_style};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use std::sync::mpsc;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn enter_submits_input_and_shift_enter_keeps_a_newline() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (actions, received) = mpsc::channel();
    handle_key(
        key(KeyCode::Char('h'), KeyModifiers::NONE),
        &mut app,
        &actions,
    )
    .unwrap();
    handle_key(key(KeyCode::Enter, KeyModifiers::SHIFT), &mut app, &actions).unwrap();
    handle_key(
        key(KeyCode::Char('i'), KeyModifiers::NONE),
        &mut app,
        &actions,
    )
    .unwrap();
    assert_eq!(app.input_text(), "h\ni");
    handle_key(key(KeyCode::Enter, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(
        received.try_recv().unwrap(),
        ChatAction::Submit("h\ni".to_owned())
    );
    assert!(app.input.is_empty());
}

#[test]
fn escape_cancels_stream_and_quits_when_idle() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (actions, received) = mpsc::channel();
    app.streaming = true;
    assert!(!handle_key(key(KeyCode::Esc, KeyModifiers::NONE), &mut app, &actions).unwrap());
    assert_eq!(received.try_recv().unwrap(), ChatAction::Cancel);
    assert!(handle_key(key(KeyCode::Esc, KeyModifiers::NONE), &mut app, &actions).unwrap());
    assert_eq!(received.try_recv().unwrap(), ChatAction::Quit);
}

#[test]
fn ctrl_c_quits_after_cancelling_an_active_request() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (actions, received) = mpsc::channel();
    app.streaming = true;

    assert!(
        handle_key(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut app,
            &actions
        )
        .unwrap()
    );
    assert_eq!(received.try_recv().unwrap(), ChatAction::Cancel);
    assert_eq!(received.try_recv().unwrap(), ChatAction::Quit);
}

#[test]
fn q_is_still_message_text_in_the_chat_composer() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.input = "hello ".chars().collect();
    app.cursor = app.input.len();
    let (actions, received) = mpsc::channel();

    assert!(
        !handle_key(
            key(KeyCode::Char('q'), KeyModifiers::NONE),
            &mut app,
            &actions
        )
        .unwrap()
    );
    assert_eq!(app.input_text(), "hello q");
    assert!(matches!(
        received.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn q_quits_while_the_chat_composer_is_unavailable() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.working = Some("fetching models".to_owned());
    let (actions, received) = mpsc::channel();

    assert!(
        handle_key(
            key(KeyCode::Char('q'), KeyModifiers::NONE),
            &mut app,
            &actions
        )
        .unwrap()
    );
    assert_eq!(received.try_recv().unwrap(), ChatAction::Cancel);
    assert_eq!(received.try_recv().unwrap(), ChatAction::Quit);
}

#[test]
fn tab_completes_a_candidate_and_updates_the_cursor() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.input = "/mo".chars().collect();
    app.cursor = app.input.len();
    app.completions = vec!["/model gpt-test".to_owned()];
    let (actions, received) = mpsc::channel();
    handle_key(key(KeyCode::Tab, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(app.input_text(), "/model gpt-test");
    assert_eq!(app.cursor, app.input.len());
    assert!(matches!(
        received.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn tab_cycles_through_candidates_sharing_the_same_prefix() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.input = "/model gpt".chars().collect();
    app.cursor = app.input.len();
    app.completions = vec!["/model gpt-fast".to_owned(), "/model gpt-pro".to_owned()];
    let (actions, _) = mpsc::channel();

    handle_key(key(KeyCode::Tab, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(app.input_text(), "/model gpt-fast");
    handle_key(key(KeyCode::Tab, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(app.input_text(), "/model gpt-pro");
    handle_key(key(KeyCode::Tab, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(app.input_text(), "/model gpt-fast");
}

#[test]
fn streamed_events_keep_editor_and_session_footer_visible() {
    let mut app = ChatApp::new(ChatConfig {
        profile: "nim".to_owned(),
        provider: "NVIDIA NIM".to_owned(),
        model: "nemotron".to_owned(),
        effort: None,
    });
    app.push_message(Role::User, "hello".to_owned());
    let (sender, events) = mpsc::channel();
    sender.send(ChatEvent::AssistantStarted).unwrap();
    sender
        .send(ChatEvent::TextDelta("Hello **there**.".to_owned()))
        .unwrap();
    sender
        .send(ChatEvent::Session {
            profile: "nim".to_owned(),
            provider: "NVIDIA NIM".to_owned(),
            model: "nemotron".to_owned(),
            effort: None,
            turns: 1,
            usage: ChatUsage {
                total_tokens: Some(42),
                ..ChatUsage::default()
            },
        })
        .unwrap();
    drain_events(&events, &mut app);
    let backend = TestBackend::new(80, VIEWPORT_HEIGHT);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| view::draw(frame, &app)).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("▎ hello"));
    assert!(rendered.contains("● Hello there."));
    assert!(rendered.contains("Waiting for response…"));
    assert!(rendered.contains("NVIDIA NIM"));
    assert!(rendered.contains("nim · NVIDIA NIM · nemotron · 42 tokens"));
    assert!(rendered.contains("Esc cancel"));
}

#[test]
fn confirmation_is_bound_to_its_request_and_defaults_to_no() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (actions, received) = mpsc::channel();
    let (sender, events) = mpsc::channel();
    sender
        .send(ChatEvent::ConfirmationRequested {
            request_id: 17,
            prompt: "Apply this plan?".to_owned(),
        })
        .unwrap();
    drain_events(&events, &mut app);
    assert!(!app.confirmation.as_ref().unwrap().approved);

    let backend = TestBackend::new(60, VIEWPORT_HEIGHT);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| view::draw(frame, &app)).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("Apply this plan?"));
    assert!(rendered.contains("[ No ]"));

    handle_key(key(KeyCode::Enter, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(
        received.try_recv().unwrap(),
        ChatAction::Confirm {
            request_id: 17,
            approved: false
        }
    );
}

#[test]
fn commands_get_ghost_text_and_headings_are_rendered_without_markers() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.input = "/he".chars().collect();
    app.cursor = app.input.len();
    app.completions = vec!["/help".to_owned()];
    assert_eq!(view::ghost_completion(&app).as_deref(), Some("lp"));
    let rendered = super::markdown::render("## Heading\n- one");
    assert_eq!(rendered[0].spans[0].content, "Heading");
    assert_eq!(rendered[1].spans[0].content, "• ");
}

#[test]
fn empty_input_keeps_the_message_placeholder_without_a_command_suggestion() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.completions = vec!["/help".to_owned()];
    let rows = render(&app, 80);
    let composer = rows
        .iter()
        .find(|row| row.contains("Type a message…"))
        .unwrap();
    assert!(!composer.contains("/help"));
}

fn render(app: &ChatApp, width: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, VIEWPORT_HEIGHT)).unwrap();
    terminal.draw(|frame| view::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn a_failed_response_is_labeled_failed_and_shows_guidance() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (sender, events) = mpsc::channel();
    sender.send(ChatEvent::AssistantStarted).unwrap();
    sender
        .send(ChatEvent::Error(
            "Could not reach Ollama at http://127.0.0.1:9.\nIs the server running? Check it with `inifra assistant configure local`.\ndetails: could not connect: Connection refused".to_owned(),
        ))
        .unwrap();
    drain_events(&events, &mut app);
    let rows = render(&app, 100);

    assert!(rows.contains(&"● request failed".to_owned()));
    assert!(!rows.iter().any(|row| row.contains("interrupted")));
    assert!(rows.contains(&"Could not reach Ollama at http://127.0.0.1:9.".to_owned()));
    assert!(rows.contains(
        &"  Is the server running? Check it with inifra assistant configure local.".to_owned()
    ));
    assert!(rows.contains(&"  details: could not connect: Connection refused".to_owned()));
}

#[test]
fn clearing_the_conversation_removes_earlier_messages_but_keeps_the_notice() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (sender, events) = mpsc::channel();
    sender.send(ChatEvent::AssistantStarted).unwrap();
    sender
        .send(ChatEvent::TextDelta("Old answer".to_owned()))
        .unwrap();
    sender.send(ChatEvent::AssistantFinished).unwrap();
    sender.send(ChatEvent::ConversationCleared).unwrap();
    sender
        .send(ChatEvent::Notice("Started a new conversation.".to_owned()))
        .unwrap();
    drain_events(&events, &mut app);

    let rows = render(&app, 80);
    assert!(!rows.iter().any(|row| row.contains("Old answer")));
    assert!(
        rows.iter()
            .any(|row| row.contains("Started a new conversation."))
    );
}

#[test]
fn a_cancelled_response_is_labeled_interrupted() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (sender, events) = mpsc::channel();
    sender.send(ChatEvent::AssistantStarted).unwrap();
    sender
        .send(ChatEvent::TextDelta("Partial".to_owned()))
        .unwrap();
    sender.send(ChatEvent::AssistantCancelled).unwrap();
    drain_events(&events, &mut app);
    let rows = render(&app, 80);
    assert!(rows.contains(&"● Partial".to_owned()));
    assert!(rows.contains(&"  interrupted".to_owned()));
}

#[test]
fn error_guidance_highlights_commands_and_mutes_details() {
    let lines = view::error_lines("Summary\nRun `inifra assistant` now.\ndetails: raw");
    assert_eq!(
        lines[0].style.fg,
        crate::presentation::tui_style(crate::presentation::Tone::Error, true).fg
    );
    let command = lines[1]
        .spans
        .iter()
        .find(|span| span.content == "inifra assistant")
        .unwrap();
    assert_eq!(
        command.style.fg,
        crate::presentation::tui_style(crate::presentation::Tone::Accent, false).fg
    );
    assert_eq!(lines[2].spans[0].content, "  details: ");
}

#[test]
fn footer_leads_with_the_profile_and_compacts_token_counts() {
    let mut app = ChatApp::new(ChatConfig {
        profile: "work".to_owned(),
        provider: "OpenAI".to_owned(),
        model: "gpt-5".to_owned(),
        effort: Some("high".to_owned()),
    });
    app.usage = ChatUsage {
        input_tokens: Some(1_234),
        output_tokens: Some(340),
        total_tokens: Some(1_574),
    };
    let rows = render(&app, 80);
    let footer = rows.last().unwrap();
    assert!(footer.starts_with("work · OpenAI · gpt-5 · high effort · 1.2k in · 340 out"));
    assert!(footer.ends_with("Ctrl+O switch"));
    assert_eq!(view::compact_count(999), "999");
    assert_eq!(view::compact_count(1_000), "1k");
    assert_eq!(view::compact_count(12_345), "12.3k");
    assert_eq!(view::compact_count(2_500_000), "2.5M");
}

#[test]
fn footer_does_not_repeat_a_profile_named_after_its_model() {
    let app = ChatApp::new(ChatConfig {
        profile: "test".to_owned(),
        provider: "OpenAI".to_owned(),
        model: "test".to_owned(),
        effort: None,
    });
    let footer = render(&app, 80).pop().unwrap();
    assert!(footer.starts_with("OpenAI · test"));
    assert_eq!(footer.matches("test").count(), 1);
}

#[test]
fn composer_is_framed_by_rules() {
    let rows = render(&ChatApp::new(ChatConfig::default()), 40);
    assert_eq!(rows.iter().filter(|row| **row == "─".repeat(40)).count(), 2);
}

#[test]
fn empty_chat_points_to_help() {
    let app = ChatApp::new(ChatConfig::default());
    let rows = render(&app, 80);
    assert!(
        rows.iter()
            .any(|row| row == "Ask about your project or browse templates. /help lists commands.")
    );
}

fn switcher_app() -> (ChatApp, mpsc::Sender<ChatEvent>, mpsc::Receiver<ChatEvent>) {
    let app = ChatApp::new(ChatConfig {
        profile: "local".to_owned(),
        provider: "Ollama".to_owned(),
        model: "qwen3:8b".to_owned(),
        effort: None,
    });
    let (sender, events) = mpsc::channel();
    sender
        .send(ChatEvent::SwitcherOpened {
            profiles: vec![
                SwitcherProfile {
                    id: "local".to_owned(),
                    detail: "Ollama · Ollama Chat".to_owned(),
                    default_model: "qwen3:8b".to_owned(),
                },
                SwitcherProfile {
                    id: "work".to_owned(),
                    detail: "OpenAI · Responses".to_owned(),
                    default_model: "gpt-5".to_owned(),
                },
            ],
            active: "local".to_owned(),
            model: "qwen3:8b".to_owned(),
            efforts: ["provider default", "low", "medium", "high"]
                .map(str::to_owned)
                .to_vec(),
            effort: None,
        })
        .unwrap();
    (app, sender, events)
}

#[test]
fn ctrl_o_asks_the_application_to_open_the_switcher() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (actions, received) = mpsc::channel();
    handle_key(
        key(KeyCode::Char('o'), KeyModifiers::CONTROL),
        &mut app,
        &actions,
    )
    .unwrap();
    assert_eq!(received.try_recv(), Ok(ChatAction::OpenSwitcher));
}

#[test]
fn switcher_loads_models_shows_them_and_applies_profile_model_and_effort() {
    let (mut app, sender, events) = switcher_app();
    assert_eq!(
        drain_events(&events, &mut app),
        [ChatAction::LoadModels("local".to_owned())]
    );
    sender
        .send(ChatEvent::ModelsListed {
            profile: "local".to_owned(),
            models: ModelList::Ready(vec!["qwen3:8b".to_owned(), "llama3.2".to_owned()]),
        })
        .unwrap();
    drain_events(&events, &mut app);

    let rows = render(&app, 100);
    assert_eq!(rows[0], "Inifra · switch model");
    assert!(rows[2].contains("Profiles") && rows[2].contains("Models · local"));
    assert!(rows[3].contains("local") && rows[3].contains("› qwen3:8b"));
    assert!(rows[4].contains("llama3.2"));
    assert!(
        rows.iter()
            .any(|row| row.contains("Effort   ‹ provider default ›"))
    );

    let (actions, received) = mpsc::channel();
    handle_key(
        key(KeyCode::Char('j'), KeyModifiers::NONE),
        &mut app,
        &actions,
    )
    .unwrap();
    handle_key(
        key(KeyCode::Char('e'), KeyModifiers::NONE),
        &mut app,
        &actions,
    )
    .unwrap();
    handle_key(
        key(KeyCode::Char('e'), KeyModifiers::NONE),
        &mut app,
        &actions,
    )
    .unwrap();
    handle_key(key(KeyCode::Enter, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(
        received.try_recv(),
        Ok(ChatAction::Switch(SwitchRequest {
            profile: "local".to_owned(),
            model: "llama3.2".to_owned(),
            effort: Some("medium".to_owned()),
            save_default: false,
        }))
    );
    assert!(app.switcher.is_none());
}

#[test]
fn switcher_requests_models_for_other_profiles_and_offers_their_default_model() {
    let (mut app, sender, events) = switcher_app();
    drain_events(&events, &mut app);
    let (actions, received) = mpsc::channel();
    handle_key(key(KeyCode::Tab, KeyModifiers::NONE), &mut app, &actions).unwrap();
    handle_key(key(KeyCode::Down, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert_eq!(
        received.try_recv(),
        Ok(ChatAction::LoadModels("work".to_owned()))
    );
    sender
        .send(ChatEvent::ModelsListed {
            profile: "work".to_owned(),
            models: ModelList::Unavailable("OPENAI_API_KEY is not set.".to_owned()),
        })
        .unwrap();
    drain_events(&events, &mut app);
    let rows = render(&app, 100);
    assert!(rows.iter().any(|row| row.contains("gpt-5   default")));
    assert!(
        rows.iter()
            .any(|row| row.contains("OPENAI_API_KEY is not set."))
    );

    handle_key(
        key(KeyCode::Char('s'), KeyModifiers::NONE),
        &mut app,
        &actions,
    )
    .unwrap();
    assert_eq!(
        received.try_recv(),
        Ok(ChatAction::Switch(SwitchRequest {
            profile: "work".to_owned(),
            model: "gpt-5".to_owned(),
            effort: None,
            save_default: true,
        }))
    );
}

#[test]
fn escape_closes_the_switcher_without_changes() {
    let (mut app, _sender, events) = switcher_app();
    drain_events(&events, &mut app);
    let (actions, received) = mpsc::channel();
    handle_key(key(KeyCode::Esc, KeyModifiers::NONE), &mut app, &actions).unwrap();
    assert!(app.switcher.is_none());
    assert!(received.try_recv().is_err());
}

#[test]
fn local_command_output_is_a_wrapped_card_with_a_gutter() {
    let mut app = ChatApp::new(ChatConfig::default());
    let (sender, events) = mpsc::channel();
    sender
        .send(ChatEvent::AssistantMessage(
            "Chat with the configured assistant, or use local commands:\n\n/effort [level]  Show or set reasoning effort: low, medium, high, default".to_owned(),
        ))
        .unwrap();
    drain_events(&events, &mut app);
    let rows = render(&app, 60);

    assert!(rows.iter().any(|row| row == "◆ Local"), "{rows:#?}");
    let start = rows.iter().position(|row| row.contains("/effort")).unwrap();
    assert!(rows[start].starts_with("▏ /effort [level]  Show or set"));
    // Wrapped description lines stay inside the card and align under the description.
    assert!(rows[start + 1].starts_with("▏                 "));
    assert!(
        rows[start..start + 2]
            .iter()
            .all(|row| row.starts_with("▏ "))
    );
    assert!(rows.iter().all(|row| row.chars().count() <= 60));
}

#[test]
fn local_output_colors_commands_actions_and_headings() {
    let lines = local::render(
        "Plan for the current project\ncreate · AGENTS.md\nalready exists · README.md\n\nFiles\n/help  Show this help",
        80,
    );
    let style_of = |row: usize, text: &str| {
        lines[row]
            .spans
            .iter()
            .find(|span| {
                let word = span.content.trim();
                !word.is_empty() && text.starts_with(word)
            })
            .unwrap_or_else(|| panic!("{text:?} missing in row {row}"))
            .style
    };

    assert_eq!(
        style_of(0, "Plan for the current project"),
        tui_style(Tone::Accent, true)
    );
    assert_eq!(style_of(1, "create"), tui_style(Tone::Success, false));
    assert_eq!(style_of(2, "already exists"), tui_style(Tone::Muted, false));
    assert_eq!(style_of(4, "Files"), tui_style(Tone::Secondary, true));
    assert_eq!(style_of(5, "/help"), tui_style(Tone::Accent, false));
    assert_eq!(
        style_of(5, "Show this help"),
        tui_style(Tone::Secondary, false)
    );
}

#[test]
fn viewport_grows_with_the_conversation_and_stops_at_the_terminal() {
    // Short conversations keep the base height.
    assert_eq!(grown_height(8, VIEWPORT_HEIGHT, 40), VIEWPORT_HEIGHT);
    // Longer ones grow in steps of rows.
    assert_eq!(grown_height(15, VIEWPORT_HEIGHT, 40), 16);
    assert_eq!(grown_height(30, 16, 40), 32);
    // It stops at the ceiling and the conversation scrolls inside from there.
    assert_eq!(grown_height(200, 32, 40), 40);
    // It never shrinks when content gets shorter.
    assert_eq!(grown_height(10, 32, 40), 32);
    // A terminal shorter than the base height still gets the base height.
    assert_eq!(grown_height(10, VIEWPORT_HEIGHT, 6), VIEWPORT_HEIGHT);
}

#[test]
fn conversation_rows_count_wrapped_lines() {
    let mut app = ChatApp::new(ChatConfig::default());
    assert_eq!(view::conversation_rows(&app, 20), 0);
    app.push_message(Role::Notice, "x".repeat(45));
    // 45 characters wrap to three rows at width 20, plus the blank separator.
    assert_eq!(view::conversation_rows(&app, 20), 4);
}

#[test]
fn replies_lead_with_a_bullet_and_hang_their_other_lines_under_it() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.push_message(
        Role::Assistant,
        "First line of the reply wraps around.\n\nSecond paragraph.".to_owned(),
    );
    let rows = render(&app, 24);

    assert!(!rows.iter().any(|row| row.contains("Inifra")));
    let first = rows
        .iter()
        .position(|row| row.starts_with("● First"))
        .unwrap();
    assert_eq!(rows[first], "● First line of the");
    assert_eq!(rows[first + 1], "  reply wraps around.");
    assert!(rows.contains(&"  Second paragraph.".to_owned()));
}

#[test]
fn user_messages_sit_on_a_tinted_block_that_fills_the_row() {
    let lines = view::user_lines("hello", 20);
    let tinted = crate::presentation::tui_surface_style();
    if tinted == ratatui::style::Style::default() {
        assert_eq!(lines.len(), 1);
        return;
    }

    // One blank tinted row above and below, and every row is full width and tinted.
    assert_eq!(lines.len(), 3);
    for line in &lines {
        assert_eq!(line.width(), 20);
        assert!(line.spans.iter().all(|span| span.style.bg == tinted.bg));
    }
}

#[test]
fn waiting_shows_an_animated_indicator_until_text_arrives() {
    let mut app = ChatApp::new(ChatConfig::default());
    app.push_message(Role::User, "hi".to_owned());
    app.start_assistant();
    let waiting = render(&app, 60).join("\n");
    assert!(waiting.contains("Thinking…"), "{waiting}");
    assert!(!waiting.contains('●'));

    app.append_delta("Hello");
    let answered = render(&app, 60).join("\n");
    assert!(answered.contains("● Hello"));
    assert!(!answered.contains("Thinking"));
}

#[test]
fn indicator_cycles_words_moves_the_spinner_and_counts_seconds() {
    use std::time::Duration;
    let text = |millis: u64, status: Option<&str>| {
        thinking::indicator(Duration::from_millis(millis), status)
            .spans
            .iter()
            .map(|span| span.content.to_string())
            .collect::<String>()
    };

    assert_eq!(text(0, None), "⠋ Thinking…");
    assert_eq!(text(80, None), "⠙ Thinking…");
    assert!(text(3_000, None).contains("Reasoning…"));
    assert!(text(2_500, None).ends_with("  2s"));
    // A named task replaces the rotating words.
    assert!(text(0, Some("Fetching models")).contains("Fetching models…"));
}

#[test]
fn command_descriptions_stack_under_the_command_on_narrow_screens() {
    let text = "Commands:\n\n/plan <id>... [--set name=value]  Prepare a generation plan; no files are written";
    let narrow = super::local::render(text, 44)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert!(narrow.contains(&"▏ /plan <id>... [--set name=value]".to_owned()));
    assert!(
        narrow
            .iter()
            .any(|row| row.starts_with("▏   Prepare a generation plan"))
    );

    let wide = super::local::render(text, 100)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert!(
        wide.iter()
            .any(|row| row.contains("/plan <id>... [--set name=value]  Prepare"))
    );
}
