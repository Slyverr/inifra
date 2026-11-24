use super::*;
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
    assert!(rendered.contains("› hello"));
    assert!(rendered.contains("Inifra ›"));
    assert!(rendered.contains("Waiting for response…"));
    assert!(rendered.contains("NVIDIA NIM"));
    assert!(rendered.contains("nim · NVIDIA NIM · nemotron · 42 tokens · Esc cancel"));
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

    assert!(rows.contains(&"Inifra ›  request failed".to_owned()));
    assert!(!rows.iter().any(|row| row.contains("interrupted")));
    assert!(rows.contains(&"Could not reach Ollama at http://127.0.0.1:9.".to_owned()));
    assert!(rows.contains(
        &"  Is the server running? Check it with inifra assistant configure local.".to_owned()
    ));
    assert!(rows.contains(&"  details: could not connect: Connection refused".to_owned()));
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
    assert!(rows.contains(&"Inifra ›  interrupted".to_owned()));
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
    });
    app.usage = ChatUsage {
        input_tokens: Some(1_234),
        output_tokens: Some(340),
        total_tokens: Some(1_574),
    };
    let rows = render(&app, 80);
    assert_eq!(
        rows.last().unwrap(),
        "work · OpenAI · gpt-5 · 1.2k in · 340 out"
    );
    assert_eq!(view::compact_count(999), "999");
    assert_eq!(view::compact_count(1_000), "1k");
    assert_eq!(view::compact_count(12_345), "12.3k");
    assert_eq!(view::compact_count(2_500_000), "2.5M");
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
