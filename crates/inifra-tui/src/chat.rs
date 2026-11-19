use crate::{
    presentation::{Tone, terminal_text, tui_style},
    terminal,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use std::{
    io,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

const VIEWPORT_HEIGHT: u16 = 18;
const INPUT_ROWS: usize = 3;

/// Initial provider and model labels shown in the chat footer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChatConfig {
    /// Configured provider or service name.
    pub provider: String,
    /// Configured model identifier.
    pub model: String,
}

/// Events sent from the TUI to the application orchestrator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatAction {
    /// Submit a user message or slash command.
    Submit(String),
    /// Cancel the active request. Sent when Esc is pressed during a response.
    Cancel,
    /// Exit the chat. Sent when Esc is pressed while idle.
    Quit,
    /// Request completion candidates for the current input.
    Complete(String),
    /// Confirm or reject one explicit plan-application prompt.
    Confirm {
        /// ID supplied with the confirmation request.
        request_id: u64,
        /// Whether the user approved the operation.
        approved: bool,
    },
}

/// Events sent from the orchestrator to the TUI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatEvent {
    /// A response has started.
    AssistantStarted,
    /// Append a piece of assistant response text.
    TextDelta(String),
    /// The response has completed normally.
    AssistantFinished,
    /// The response was cancelled by the user.
    AssistantCancelled,
    /// Replace the current session footer details.
    Session {
        /// Provider or service label.
        provider: String,
        /// Active model identifier.
        model: String,
        /// Number of completed user requests in this session.
        turns: u64,
        /// Cumulative token usage reported by the provider.
        usage: ChatUsage,
    },
    /// Set the full strings used for Tab completion and the ghost hint.
    Completions(Vec<String>),
    /// Show locally generated command output as an Inifra response.
    AssistantMessage(String),
    /// Add a non-error status line to the conversation.
    Notice(String),
    /// Mark a non-chat operation as active and pause input until it completes.
    WorkingStarted(String),
    /// Resume input after a non-chat operation completes.
    WorkingFinished,
    /// Add a visible error to the conversation.
    Error(String),
    /// Pause chat input and ask for an explicit Yes/No decision.
    ConfirmationRequested {
        /// ID returned with the user's confirmation decision.
        request_id: u64,
        /// Concise description of the pending operation.
        prompt: String,
    },
}

/// Token counts reported by the provider, when available.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChatUsage {
    /// Cumulative input-token count.
    pub input_tokens: Option<u64>,
    /// Cumulative output-token count.
    pub output_tokens: Option<u64>,
    /// Cumulative total-token count.
    pub total_tokens: Option<u64>,
}

/// How the user left the chat interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatExit {
    /// The user pressed Esc while no request was active.
    Quit,
}

/// Runs the inline assistant conversation view.
///
/// The application owns all provider and command behavior. It sends normalized events through
/// `events` and receives user actions through `actions`; this crate has no provider or core
/// dependency. While a response streams, the event loop continues to redraw the conversation,
/// keep the editor and footer visible, and accepts Esc cancellation.
///
/// # Errors
/// Returns an error when the terminal cannot be initialized, drawn, or read.
pub fn run_chat(
    config: ChatConfig,
    actions: Sender<ChatAction>,
    events: Receiver<ChatEvent>,
) -> io::Result<ChatExit> {
    let mut app = ChatApp::new(config);
    let (_restore, mut terminal) = terminal::inline(VIEWPORT_HEIGHT)?;

    loop {
        drain_events(&events, &mut app);
        terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(40))? {
            match event::read()? {
                Event::Resize(_, _) => terminal.autoresize()?,
                Event::Key(key)
                    if key.kind != KeyEventKind::Release
                        && handle_key(key, &mut app, &actions)? =>
                {
                    let _ = terminal.clear();
                    return Ok(ChatExit::Quit);
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    User,
    Assistant,
    Notice,
    Error,
}

#[derive(Clone, Debug)]
struct Message {
    role: Role,
    text: String,
    incomplete: bool,
}

struct Confirmation {
    request_id: u64,
    prompt: String,
    approved: bool,
}

struct ChatApp {
    provider: String,
    model: String,
    turns: u64,
    usage: ChatUsage,
    messages: Vec<Message>,
    input: Vec<char>,
    cursor: usize,
    completions: Vec<String>,
    completion_index: Option<usize>,
    completion_prefix: Option<String>,
    streaming: bool,
    working: Option<String>,
    confirmation: Option<Confirmation>,
    scroll_from_bottom: u16,
}

impl ChatApp {
    fn new(config: ChatConfig) -> Self {
        Self {
            provider: config.provider,
            model: config.model,
            turns: 0,
            usage: ChatUsage::default(),
            messages: Vec::new(),
            input: Vec::new(),
            cursor: 0,
            completions: Vec::new(),
            completion_index: None,
            completion_prefix: None,
            streaming: false,
            working: None,
            confirmation: None,
            scroll_from_bottom: 0,
        }
    }

    fn input_text(&self) -> String {
        self.input.iter().collect()
    }

    fn push_message(&mut self, role: Role, text: String) {
        self.messages.push(Message {
            role,
            text,
            incomplete: false,
        });
        self.scroll_from_bottom = 0;
    }

    fn start_assistant(&mut self) {
        self.streaming = true;
        self.push_message(Role::Assistant, String::new());
        if let Some(message) = self.messages.last_mut() {
            message.incomplete = true;
        }
    }

    fn append_delta(&mut self, text: &str) {
        if !self.streaming {
            self.start_assistant();
        }
        if let Some(message) = self.messages.last_mut()
            && message.role == Role::Assistant
        {
            message.text.push_str(text);
        }
        self.scroll_from_bottom = 0;
    }

    fn finish_assistant(&mut self, incomplete: bool) {
        self.streaming = false;
        if let Some(message) = self
            .messages
            .iter_mut()
            .rev()
            .find(|message| message.role == Role::Assistant)
        {
            message.incomplete = incomplete;
        }
    }
}

fn drain_events(events: &Receiver<ChatEvent>, app: &mut ChatApp) {
    while let Ok(event) = events.try_recv() {
        match event {
            ChatEvent::AssistantStarted => app.start_assistant(),
            ChatEvent::TextDelta(text) => app.append_delta(&text),
            ChatEvent::AssistantFinished => app.finish_assistant(false),
            ChatEvent::AssistantCancelled => app.finish_assistant(true),
            ChatEvent::Session {
                provider,
                model,
                turns,
                usage,
            } => {
                app.provider = provider;
                app.model = model;
                app.turns = turns;
                app.usage = usage;
            }
            ChatEvent::Completions(completions) => {
                app.completions = completions;
                clear_completion(app);
            }
            ChatEvent::AssistantMessage(text) => app.push_message(Role::Assistant, text),
            ChatEvent::Notice(text) => app.push_message(Role::Notice, text),
            ChatEvent::WorkingStarted(status) => app.working = Some(status),
            ChatEvent::WorkingFinished => app.working = None,
            ChatEvent::Error(text) => {
                if app.streaming {
                    app.finish_assistant(true);
                }
                app.working = None;
                app.push_message(Role::Error, text);
            }
            ChatEvent::ConfirmationRequested { request_id, prompt } => {
                app.confirmation = Some(Confirmation {
                    request_id,
                    prompt,
                    approved: false,
                });
            }
        }
    }
}

fn handle_key(key: KeyEvent, app: &mut ChatApp, actions: &Sender<ChatAction>) -> io::Result<bool> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if let Some(confirmation) = &mut app.confirmation {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                let request_id = confirmation.request_id;
                app.confirmation = None;
                let _ = actions.send(ChatAction::Confirm {
                    request_id,
                    approved: false,
                });
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => confirmation.approved = true,
            KeyCode::Left | KeyCode::Char('h') => confirmation.approved = false,
            KeyCode::Right | KeyCode::Char('l') => confirmation.approved = true,
            KeyCode::Enter => {
                let request_id = confirmation.request_id;
                let approved = confirmation.approved;
                app.confirmation = None;
                let _ = actions.send(ChatAction::Confirm {
                    request_id,
                    approved,
                });
            }
            _ => {}
        }
        return Ok(false);
    }

    if key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && control) {
        if app.streaming || app.working.is_some() {
            if app.streaming {
                app.finish_assistant(true);
            }
            app.working = None;
            let _ = actions.send(ChatAction::Cancel);
        } else {
            let _ = actions.send(ChatAction::Quit);
            return Ok(true);
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.input.insert(app.cursor, '\n');
            app.cursor += 1;
            clear_completion(app);
        }
        KeyCode::Enter if !app.streaming && app.working.is_none() => {
            let text = app.input_text();
            if !text.trim().is_empty() {
                app.push_message(Role::User, text.clone());
                app.input.clear();
                app.cursor = 0;
                clear_completion(app);
                actions
                    .send(ChatAction::Submit(text))
                    .map_err(channel_error)?;
            }
        }
        KeyCode::Tab if !app.streaming && app.working.is_none() => complete_input(app, actions)?,
        KeyCode::Backspace if !app.streaming && app.working.is_none() && app.cursor > 0 => {
            app.input.remove(app.cursor - 1);
            app.cursor -= 1;
            clear_completion(app);
        }
        KeyCode::Delete
            if !app.streaming && app.working.is_none() && app.cursor < app.input.len() =>
        {
            app.input.remove(app.cursor);
            clear_completion(app);
        }
        KeyCode::Left if !app.streaming && app.working.is_none() => {
            app.cursor = app.cursor.saturating_sub(1);
            clear_completion(app);
        }
        KeyCode::Right if !app.streaming && app.working.is_none() => {
            app.cursor = (app.cursor + 1).min(app.input.len());
            clear_completion(app);
        }
        KeyCode::Home if !app.streaming && app.working.is_none() => {
            app.cursor = line_start(&app.input, app.cursor);
            clear_completion(app);
        }
        KeyCode::End if !app.streaming && app.working.is_none() => {
            app.cursor = line_end(&app.input, app.cursor);
            clear_completion(app);
        }
        KeyCode::Char('a') if control && !app.streaming && app.working.is_none() => {
            app.cursor = line_start(&app.input, app.cursor);
            clear_completion(app);
        }
        KeyCode::Char('e') if control && !app.streaming && app.working.is_none() => {
            app.cursor = line_end(&app.input, app.cursor);
            clear_completion(app);
        }
        KeyCode::Char('u') if control && !app.streaming && app.working.is_none() => {
            let start = line_start(&app.input, app.cursor);
            app.input.drain(start..app.cursor);
            app.cursor = start;
            clear_completion(app);
        }
        KeyCode::Char('w') if control && !app.streaming && app.working.is_none() => {
            let start = app.input[..app.cursor]
                .iter()
                .rposition(|character| character.is_whitespace())
                .map_or(0, |index| index + 1);
            app.input.drain(start..app.cursor);
            app.cursor = start;
            clear_completion(app);
        }
        KeyCode::Up if app.scroll_from_bottom > 0 => {
            app.scroll_from_bottom = app.scroll_from_bottom.saturating_add(1)
        }
        KeyCode::Up if !app.streaming && app.working.is_none() && app.input.contains(&'\n') => {
            move_vertical(app, -1);
            clear_completion(app);
        }
        KeyCode::Down if app.scroll_from_bottom > 0 => {
            app.scroll_from_bottom = app.scroll_from_bottom.saturating_sub(1)
        }
        KeyCode::Down if !app.streaming && app.working.is_none() && app.input.contains(&'\n') => {
            move_vertical(app, 1);
            clear_completion(app);
        }
        KeyCode::PageUp => app.scroll_from_bottom = app.scroll_from_bottom.saturating_add(5),
        KeyCode::PageDown => app.scroll_from_bottom = app.scroll_from_bottom.saturating_sub(5),
        KeyCode::Char(character)
            if !app.streaming
                && app.working.is_none()
                && !control
                && !key.modifiers.contains(KeyModifiers::ALT)
                && !character.is_control() =>
        {
            app.input.insert(app.cursor, character);
            app.cursor += 1;
            clear_completion(app);
        }
        _ => {}
    }
    Ok(false)
}

fn channel_error<T>(_: std::sync::mpsc::SendError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "assistant session has stopped")
}

fn clear_completion(app: &mut ChatApp) {
    app.completion_index = None;
    app.completion_prefix = None;
}

fn complete_input(app: &mut ChatApp, actions: &Sender<ChatAction>) -> io::Result<()> {
    let input = app.input_text();
    let prefix = app
        .completion_prefix
        .clone()
        .unwrap_or_else(|| input.clone());
    let matching = app
        .completions
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.starts_with(&prefix))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if !matching.is_empty() {
        let selected = app
            .completion_index
            .and_then(|current| matching.iter().position(|index| *index == current))
            .map_or(0, |position| (position + 1) % matching.len());
        let candidate_index = matching[selected];
        let candidate = &app.completions[candidate_index];
        app.input = candidate.chars().collect();
        app.cursor = app.input.len();
        app.completion_index = Some(candidate_index);
        app.completion_prefix = Some(prefix);
    } else {
        actions
            .send(ChatAction::Complete(input))
            .map_err(channel_error)?;
    }
    Ok(())
}

fn line_start(input: &[char], cursor: usize) -> usize {
    input[..cursor]
        .iter()
        .rposition(|character| *character == '\n')
        .map_or(0, |index| index + 1)
}

fn line_end(input: &[char], cursor: usize) -> usize {
    cursor
        + input[cursor..]
            .iter()
            .position(|character| *character == '\n')
            .unwrap_or(input.len() - cursor)
}

fn move_vertical(app: &mut ChatApp, direction: i32) {
    let start = line_start(&app.input, app.cursor);
    let column = app.cursor - start;
    let previous_end = start.saturating_sub(1);
    if direction < 0 {
        if start == 0 {
            return;
        }
        let previous_start = line_start(&app.input, previous_end);
        app.cursor = (previous_start + column).min(previous_end);
    } else {
        let end = line_end(&app.input, app.cursor);
        if end == app.input.len() {
            return;
        }
        app.cursor = (end + 1 + column).min(line_end(&app.input, end + 1));
    }
}

fn draw(frame: &mut Frame<'_>, app: &ChatApp) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(INPUT_ROWS as u16),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_messages(frame, app, chunks[0]);
    frame.render_widget(Paragraph::new(""), chunks[1]);
    draw_input(frame, app, chunks[2]);
    frame.render_widget(Paragraph::new(""), chunks[3]);
    draw_footer(frame, app, chunks[4]);

    if app.confirmation.is_none() && !app.streaming && app.working.is_none() && chunks[2].height > 0
    {
        let (row, column) = cursor_position(app, chunks[2].width.saturating_sub(2));
        let visible_start =
            wrapped_input_lines(&app.input, chunks[2].width.saturating_sub(2) as usize)
                .len()
                .saturating_sub(INPUT_ROWS) as u16;
        if row >= visible_start && row - visible_start < chunks[2].height {
            frame
                .set_cursor_position((chunks[2].x + 2 + column, chunks[2].y + row - visible_start));
        }
    }
}

fn draw_messages(frame: &mut Frame<'_>, app: &ChatApp, area: ratatui::layout::Rect) {
    let mut lines = Vec::new();
    for message in &app.messages {
        match message.role {
            Role::User => {
                let mut text_lines = message.text.lines();
                if let Some(first) = text_lines.next() {
                    lines.push(Line::from(vec![
                        Span::styled("› ", tui_style(Tone::Accent, true)),
                        Span::styled(terminal_text(first), Style::default().fg(Color::White)),
                    ]));
                }
                lines.extend(
                    text_lines.map(|line| Line::from(format!("  {}", terminal_text(line)))),
                );
            }
            Role::Assistant => {
                let label = if message.incomplete {
                    "Inifra › · incomplete"
                } else {
                    "Inifra ›"
                };
                lines.push(Line::styled(label, tui_style(Tone::Accent, true)));
                lines.extend(super::markdown::render(&message.text));
            }
            Role::Notice => lines.push(Line::styled(
                terminal_text(&message.text),
                tui_style(Tone::Muted, false),
            )),
            Role::Error => lines.push(Line::styled(
                terminal_text(&message.text),
                tui_style(Tone::Error, true),
            )),
        }
        lines.push(Line::from(""));
    }
    let text = Text::from(lines);
    let width = usize::from(area.width.max(1));
    let total = text
        .lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width))
        .sum::<usize>();
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let max_scroll = total.saturating_sub(area.height as usize) as u16;
    let scroll = max_scroll.saturating_sub(app.scroll_from_bottom);
    frame.render_widget(paragraph.scroll((scroll, 0)), area);
}

fn draw_input(frame: &mut Frame<'_>, app: &ChatApp, area: ratatui::layout::Rect) {
    if let Some(confirmation) = &app.confirmation {
        let choice = if confirmation.approved {
            "  No     [ Yes ]"
        } else {
            "[ No ]    Yes"
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    terminal_text(&confirmation.prompt),
                    tui_style(Tone::Warning, true),
                ),
                Line::from(choice).style(tui_style(Tone::Accent, true)),
                Line::from("←/→ choose · Enter confirm · Esc cancel"),
            ]),
            area,
        );
        return;
    }

    let mut visual = wrapped_input_lines(&app.input, area.width.saturating_sub(2) as usize);
    if visual.is_empty() {
        visual.push(Vec::new());
    }
    let ghost = ghost_completion(app);
    if app.input.is_empty() && ghost.is_none() && visual.len() == 1 {
        let mut spans = vec![Span::styled("› ", tui_style(Tone::Accent, true))];
        spans.push(Span::styled(
            if app.streaming || app.working.is_some() {
                app.working.as_deref().unwrap_or("Waiting for response…")
            } else {
                "Type a message…"
            },
            tui_style(Tone::Muted, false),
        ));
        frame.render_widget(
            Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let visible_start = visual.len().saturating_sub(INPUT_ROWS);
    visual = visual.into_iter().skip(visible_start).collect();
    let input_chars = &app.input;
    let command_end = input_chars
        .iter()
        .position(|character| character.is_whitespace())
        .unwrap_or(input_chars.len());
    let mut lines = Vec::new();
    let mut consumed = 0usize;
    for (row, chars) in visual.iter().enumerate() {
        let mut spans = Vec::new();
        if row == 0 {
            spans.push(Span::styled("› ", tui_style(Tone::Accent, true)));
        } else {
            spans.push(Span::raw("  "));
        }
        for character in chars {
            let style = if input_chars.first() == Some(&'/') && consumed < command_end {
                tui_style(Tone::Accent, true)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(terminal_text(&character.to_string()), style));
            consumed += 1;
        }
        if row == 0
            && let Some(ghost) = &ghost
        {
            spans.push(Span::styled(
                terminal_text(ghost),
                tui_style(Tone::Muted, false),
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn wrapped_input_lines(input: &[char], width: usize) -> Vec<Vec<char>> {
    let width = width.max(1);
    let mut lines = vec![Vec::new()];
    for character in input {
        if *character == '\n' {
            lines.push(Vec::new());
        } else {
            if lines.last().is_some_and(|line| line.len() >= width) {
                lines.push(Vec::new());
            }
            lines.last_mut().expect("input has a line").push(*character);
        }
    }
    lines
}

fn ghost_completion(app: &ChatApp) -> Option<String> {
    let input = app.input_text();
    app.completions
        .iter()
        .find(|candidate| candidate.starts_with(&input) && candidate.len() > input.len())
        .map(|candidate| candidate[input.len()..].to_owned())
}

fn cursor_position(app: &ChatApp, width: u16) -> (u16, u16) {
    let width = usize::from(width.max(1));
    let mut row = 0usize;
    let mut column = 0usize;
    for character in app.input.iter().take(app.cursor) {
        if *character == '\n' || column >= width {
            row += 1;
            column = 0;
            if *character == '\n' {
                continue;
            }
        }
        column += 1;
    }
    (
        row.min(u16::MAX as usize) as u16,
        column.min(u16::MAX as usize) as u16,
    )
}

fn draw_footer(frame: &mut Frame<'_>, app: &ChatApp, area: ratatui::layout::Rect) {
    let mut details = Vec::<Span<'_>>::new();
    if !app.provider.is_empty() {
        details.push(Span::styled(
            terminal_text(&app.provider),
            tui_style(Tone::Muted, false),
        ));
    }
    if !app.model.is_empty() {
        if !details.is_empty() {
            details.push(Span::styled(" · ", tui_style(Tone::Muted, false)));
        }
        details.push(Span::styled(
            terminal_text(&app.model),
            Style::default().fg(Color::White),
        ));
    }
    if app.turns > 0 {
        if !details.is_empty() {
            details.push(Span::styled(" · ", tui_style(Tone::Muted, false)));
        }
        details.push(Span::styled(
            format!("{} turns", app.turns),
            tui_style(Tone::Muted, false),
        ));
    }
    let usage = app
        .usage
        .total_tokens
        .or_else(|| Some(app.usage.input_tokens? + app.usage.output_tokens?));
    if let Some(tokens) = usage {
        if !details.is_empty() {
            details.push(Span::styled(" · ", tui_style(Tone::Muted, false)));
        }
        details.push(Span::styled(
            format!("{tokens} tokens"),
            tui_style(Tone::Muted, false),
        ));
    }
    if app.streaming || app.working.is_some() {
        if !details.is_empty() {
            details.push(Span::styled(" · ", tui_style(Tone::Muted, false)));
        }
        details.push(Span::styled(
            app.working.as_deref().unwrap_or("streaming · Esc cancels"),
            tui_style(Tone::Accent, false),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(details)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
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
        terminal.draw(|frame| draw(frame, &app)).unwrap();
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
        assert!(rendered.contains("42 tokens"));
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
        terminal.draw(|frame| draw(frame, &app)).unwrap();
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
        assert_eq!(ghost_completion(&app).as_deref(), Some("lp"));
        let rendered = super::super::markdown::render("## Heading\n- one");
        assert_eq!(rendered[0].spans[0].content, "Heading");
        assert_eq!(rendered[1].spans[0].content, "• ");
    }
}
