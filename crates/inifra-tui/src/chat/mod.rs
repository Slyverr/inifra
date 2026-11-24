//! Inline assistant conversation and Markdown rendering.

mod markdown;
mod view;

use crate::terminal;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{
    io,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

const EMPTY_VIEWPORT_HEIGHT: u16 = 7;
const VIEWPORT_HEIGHT: u16 = 14;
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
    /// Exit the chat. Sent when Esc is pressed while idle or Ctrl+C is pressed; q also exits
    /// while the composer is unavailable.
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
    /// The user left the chat while no request was active.
    Quit,
}

/// Runs the inline assistant conversation view.
///
/// The application owns all provider and command behavior. It sends normalized events through
/// `events` and receives user actions through `actions`; this crate has no provider or core
/// dependency. While a response streams, the event loop continues to redraw the conversation,
/// keep the editor and footer visible, and accepts Esc cancellation. Ctrl+C quits; q quits while
/// a non-chat operation is active and remains available as message text while the composer is active.
///
/// # Errors
/// Returns an error when the terminal cannot be initialized, drawn, or read.
pub fn run_chat(
    config: ChatConfig,
    actions: Sender<ChatAction>,
    events: Receiver<ChatEvent>,
) -> io::Result<ChatExit> {
    let mut app = ChatApp::new(config);
    let mut terminal = terminal::InlineTerminal::new(EMPTY_VIEWPORT_HEIGHT)?;

    loop {
        drain_events(&events, &mut app);

        let next_viewport_height = if app.messages.is_empty() {
            EMPTY_VIEWPORT_HEIGHT
        } else {
            VIEWPORT_HEIGHT
        };
        if terminal.height() != next_viewport_height {
            terminal.set_height(next_viewport_height)?;
        }

        terminal.draw(|frame| view::draw(frame, &app))?;
        if event::poll(Duration::from_millis(40))? {
            match event::read()? {
                Event::Resize(_, _) => terminal.autoresize()?,
                Event::Key(key)
                    if key.kind != KeyEventKind::Release
                        && handle_key(key, &mut app, &actions)? =>
                {
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
    let quit = (key.code == KeyCode::Char('c') && control)
        || (key.code == KeyCode::Char('q')
            && key.modifiers.is_empty()
            && (app.streaming || app.working.is_some()));
    if quit {
        if let Some(confirmation) = app.confirmation.take() {
            let _ = actions.send(ChatAction::Confirm {
                request_id: confirmation.request_id,
                approved: false,
            });
        }
        if app.streaming || app.working.is_some() {
            if app.streaming {
                app.finish_assistant(true);
            }
            app.working = None;
            let _ = actions.send(ChatAction::Cancel);
        }
        let _ = actions.send(ChatAction::Quit);
        return Ok(true);
    }

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
            KeyCode::Char('q') if key.modifiers.is_empty() => {
                let _ = actions.send(ChatAction::Quit);
                return Ok(true);
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

    if key.code == KeyCode::Esc {
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

#[cfg(test)]
mod tests;
