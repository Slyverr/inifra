use crate::{
    presentation::{Tone, terminal_text, tui_style},
    terminal,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

const FORM_HEIGHT: u16 = 7;
const CONFIRM_HEIGHT: u16 = 6;

/// A single-line field in an inline Ratatui form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextField {
    /// Stable key used to return the submitted value.
    pub name: String,
    /// Prompt displayed above the input line.
    pub label: String,
    /// Whether an empty or whitespace-only value is rejected.
    pub required: bool,
    /// Value shown initially and returned when left unchanged.
    pub default: Option<String>,
}

/// Result of completing or cancelling an inline form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormOutcome {
    /// Submitted values indexed by each field's `name`.
    Submitted(BTreeMap<String, String>),
    /// The user cancelled without submitting.
    Cancelled,
}

/// Collects values in a compact inline form using the shared terminal UI.
///
/// Enter or Tab advances through fields. Shift+Tab returns to the previous field, and Esc cancels.
///
/// # Errors
/// Returns an I/O error if the terminal cannot be initialized, drawn, or read. Duplicate field
/// names are rejected before the terminal is initialized.
pub fn run_form(title: &str, fields: Vec<TextField>) -> io::Result<FormOutcome> {
    let mut names = BTreeSet::new();
    if fields.iter().any(|field| {
        field.name.trim().is_empty()
            || field.label.trim().is_empty()
            || !names.insert(field.name.as_str())
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "form fields need nonempty names and labels, with unique names",
        ));
    }
    if fields.is_empty() {
        return Ok(FormOutcome::Submitted(BTreeMap::new()));
    }

    let (_restore, mut terminal) = terminal::inline(FORM_HEIGHT)?;
    let mut app = FormApp::new(title, fields);
    loop {
        terminal.draw(|frame| draw_form(frame, &app))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = app.handle(key) {
                    let _ = terminal.clear();
                    return Ok(outcome);
                }
            }
            _ => {}
        }
    }
}

/// Asks a yes/no question in a compact inline Ratatui screen.
///
/// Arrow keys or `h`/`l` switch the choice. Enter confirms and Esc cancels.
///
/// # Errors
/// Returns an I/O error if the terminal cannot be initialized, drawn, or read.
pub fn run_confirmation(
    title: &str,
    question: &str,
    default_yes: bool,
) -> io::Result<Option<bool>> {
    let (_restore, mut terminal) = terminal::inline(CONFIRM_HEIGHT)?;
    let mut yes = default_yes;
    loop {
        terminal.draw(|frame| draw_confirmation(frame, title, question, yes))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc => {
                    let _ = terminal.clear();
                    return Ok(None);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let _ = terminal.clear();
                    return Ok(None);
                }
                KeyCode::Left | KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('k') => {
                    yes = true;
                }
                KeyCode::Right | KeyCode::Down | KeyCode::Char('l') | KeyCode::Char('j') => {
                    yes = false;
                }
                KeyCode::Char('y') => yes = true,
                KeyCode::Char('n') => yes = false,
                KeyCode::Enter => {
                    let _ = terminal.clear();
                    return Ok(Some(yes));
                }
                _ => {}
            },
            _ => {}
        }
    }
}

struct FormApp<'a> {
    title: &'a str,
    fields: Vec<TextField>,
    values: BTreeMap<String, String>,
    index: usize,
    input: String,
    cursor: usize,
    error: Option<String>,
}

impl<'a> FormApp<'a> {
    fn new(title: &'a str, fields: Vec<TextField>) -> Self {
        let input = fields[0].default.clone().unwrap_or_default();
        let cursor = input.chars().count();
        Self {
            title,
            fields,
            values: BTreeMap::new(),
            index: 0,
            cursor,
            input,
            error: None,
        }
    }

    fn handle(&mut self, key: KeyEvent) -> Option<FormOutcome> {
        if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Some(FormOutcome::Cancelled);
        }
        match key.code {
            KeyCode::BackTab => return self.move_field(false),
            KeyCode::Tab => return self.move_field(true),
            KeyCode::Enter => return self.move_field(true),
            KeyCode::Left if self.cursor > 0 => self.cursor -= 1,
            KeyCode::Right if self.cursor < self.input.chars().count() => self.cursor += 1,
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = 0;
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = self.input.chars().count();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                self.cursor = 0;
                self.error = None;
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(character);
                self.error = None;
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.remove_at_cursor();
                }
                self.error = None;
            }
            KeyCode::Delete => {
                if self.cursor < self.input.chars().count() {
                    self.remove_at_cursor();
                }
                self.error = None;
            }
            _ => {}
        }
        None
    }

    fn move_field(&mut self, forward: bool) -> Option<FormOutcome> {
        let field = &self.fields[self.index];
        let value = if self.input.is_empty() {
            field.default.clone().unwrap_or_default()
        } else {
            self.input.clone()
        };
        if forward && field.required && value.trim().is_empty() {
            self.error = Some("A value is required.".to_owned());
            return None;
        }

        self.values.insert(field.name.clone(), value);
        self.error = None;
        let next = if forward {
            self.index + 1
        } else {
            self.index.saturating_sub(1)
        };
        if forward && next == self.fields.len() {
            return Some(FormOutcome::Submitted(std::mem::take(&mut self.values)));
        }
        if !forward && self.index == 0 {
            self.values.remove(&field.name);
            return None;
        }
        self.index = next;
        let field = &self.fields[self.index];
        self.input = self
            .values
            .remove(&field.name)
            .or_else(|| field.default.clone())
            .unwrap_or_default();
        self.cursor = self.input.chars().count();
        None
    }

    fn insert(&mut self, character: char) {
        let byte_index = self
            .input
            .char_indices()
            .nth(self.cursor)
            .map_or(self.input.len(), |(index, _)| index);
        self.input.insert(byte_index, character);
        self.cursor += 1;
    }

    fn remove_at_cursor(&mut self) {
        let byte_index = self
            .input
            .char_indices()
            .nth(self.cursor)
            .map_or(self.input.len(), |(index, _)| index);
        if byte_index < self.input.len() {
            let next = self.input[byte_index..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or_default();
            self.input.replace_range(byte_index..byte_index + next, "");
        }
    }
}

fn draw_form(frame: &mut Frame<'_>, app: &FormApp<'_>) {
    let area = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new(terminal_text(app.title)).style(tui_style(Tone::Accent, true)),
        area[0],
    );
    let field = &app.fields[app.index];
    let requirement = if field.required {
        "required"
    } else {
        "optional"
    };
    let default = field
        .default
        .as_ref()
        .map(|value| format!(" · default: {}", terminal_text(value)))
        .unwrap_or_default();
    let input_width = area[1].width.saturating_sub(2);
    let (input, cursor_column) = visible_input(&app.input, app.cursor, input_width);
    let body = vec![
        Line::from(format!(
            "{} ({requirement}{default})",
            terminal_text(&field.label)
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("› ", tui_style(Tone::Accent, false)),
            Span::raw(input),
        ]),
    ];
    frame.render_widget(Paragraph::new(body), area[1]);
    let footer = if let Some(error) = &app.error {
        vec![
            Line::styled(terminal_text(error), tui_style(Tone::Error, false)),
            Line::from("Enter/Tab next · Shift+Tab previous · Esc cancel"),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from("Enter/Tab next · Shift+Tab previous · Esc cancel"),
        ]
    };
    frame.render_widget(Paragraph::new(footer), area[2]);
    let x = area[1].x.saturating_add(2).saturating_add(cursor_column);
    if x < area[1].right() && area[1].height > 2 {
        frame.set_cursor_position((x, area[1].y + 2));
    }
}

fn visible_input(input: &str, cursor: usize, width: u16) -> (String, u16) {
    let characters = input.chars().collect::<Vec<_>>();
    let cursor = cursor.min(characters.len());
    let width = usize::from(width);
    if width == 0 {
        return (String::new(), 0);
    }

    let mut start = 0;
    while start < cursor {
        let before_cursor = characters[start..cursor].iter().collect::<String>();
        let available = width.saturating_sub(usize::from(start > 0));
        let cursor_width = Line::from(before_cursor.as_str()).width();
        if cursor_width < width && cursor_width <= available {
            break;
        }
        start += 1;
    }
    let mut prefix = String::new();
    if start > 0 {
        prefix.push('…');
    }

    let available = width.saturating_sub(usize::from(start > 0));
    let mut visible = String::new();
    let mut visible_width = 0;
    for character in &characters[start..] {
        let next_width = Line::from(character.to_string()).width();
        let next_width = visible_width + next_width;
        if next_width > available {
            break;
        }
        visible.push(*character);
        visible_width = next_width;
    }
    let cursor_text = characters[start..cursor].iter().collect::<String>();
    let cursor_column = Line::from(cursor_text.as_str()).width() + usize::from(start > 0);
    prefix.push_str(&visible);
    let _ = visible_width;
    (
        terminal_text(&prefix),
        u16::try_from(cursor_column.min(width.saturating_sub(1))).unwrap_or(u16::MAX),
    )
}

fn draw_confirmation(frame: &mut Frame<'_>, title: &str, question: &str, yes: bool) {
    let area = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new(terminal_text(title)).style(tui_style(Tone::Accent, true)),
        area[0],
    );
    let choice = if yes {
        Line::from(vec![
            Span::styled("[ Yes ]", tui_style(Tone::Accent, true)),
            Span::raw("   "),
            Span::styled("No", tui_style(Tone::Muted, false)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Yes", tui_style(Tone::Muted, false)),
            Span::raw("   "),
            Span::styled("[ No ]", tui_style(Tone::Accent, true)),
        ])
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(terminal_text(question)),
            Line::from(""),
            choice,
        ]),
        area[1],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from("←/→ or h/l choose · Enter confirm · Esc cancel"),
        ]),
        area[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    fn field(name: &str, required: bool, default: Option<&str>) -> TextField {
        TextField {
            name: name.to_owned(),
            label: name.to_owned(),
            required,
            default: default.map(str::to_owned),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn form_returns_submitted_values_by_field_name() {
        let mut app = FormApp::new(
            "Create template",
            vec![
                field("category", true, Some("custom")),
                field("description", true, None),
            ],
        );
        assert_eq!(app.input, "custom");
        app.handle(key(KeyCode::Enter));
        for character in "Useful template".chars() {
            app.handle(key(KeyCode::Char(character)));
        }
        assert_eq!(
            app.handle(key(KeyCode::Enter)),
            Some(FormOutcome::Submitted(BTreeMap::from([
                ("category".to_owned(), "custom".to_owned()),
                ("description".to_owned(), "Useful template".to_owned()),
            ])))
        );
    }

    #[test]
    fn tab_advances_fields_and_submits_the_last_value() {
        let mut app = FormApp::new(
            "Create template",
            vec![
                field("category", true, Some("personal")),
                field("description", true, None),
            ],
        );
        assert_eq!(app.handle(key(KeyCode::Tab)), None);
        for character in "Useful template".chars() {
            app.handle(key(KeyCode::Char(character)));
        }
        assert_eq!(
            app.handle(key(KeyCode::Tab)),
            Some(FormOutcome::Submitted(BTreeMap::from([
                ("category".to_owned(), "personal".to_owned()),
                ("description".to_owned(), "Useful template".to_owned()),
            ])))
        );
    }

    #[test]
    fn form_supports_cursor_editing_in_multibyte_text() {
        let mut app = FormApp::new("Create template", vec![field("name", true, Some("café"))]);
        app.cursor = 3;
        app.handle(key(KeyCode::Backspace));
        app.handle(key(KeyCode::Char('o')));
        assert_eq!(app.input, "caoé");
        app.handle(key(KeyCode::Home));
        app.handle(key(KeyCode::Delete));
        assert_eq!(app.input, "aoé");
    }

    #[test]
    fn visible_input_keeps_cursor_in_narrow_terminal() {
        let (visible, cursor) = visible_input("some-long-template-id", 20, 8);
        assert!(visible.chars().count() <= 8);
        assert!(cursor < 8);
        assert!(visible.starts_with('…'));
    }

    #[test]
    fn form_rejects_required_empty_values_and_cancels_without_submission() {
        let mut app = FormApp::new("Create template", vec![field("description", true, None)]);
        assert_eq!(app.handle(key(KeyCode::Enter)), None);
        assert_eq!(app.error.as_deref(), Some("A value is required."));
        assert_eq!(app.handle(key(KeyCode::Esc)), Some(FormOutcome::Cancelled));
    }

    #[test]
    fn confirmation_draws_without_layout_overflow() {
        let backend = TestBackend::new(80, CONFIRM_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_confirmation(frame, "Create template", "Create 'example'?", false))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Create 'example'?"));
        assert!(text.contains("[ No ]"));
    }
}
