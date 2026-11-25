use super::{
    InteractionOutcome,
    chrome::{Heading, essential, footer, hint},
};
use crate::{
    line_edit,
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
    fmt, io,
    rc::Rc,
};
use unicode_width::UnicodeWidthStr;

const CONFIRM_FIXED_ROWS: u16 = 5;

/// Checks a field value before the form advances; the error is shown in place of the hint.
pub type FieldCheck = Rc<dyn Fn(&str) -> Result<(), String>>;

/// A single-line field in an inline Ratatui form.
#[derive(Clone)]
pub struct TextField {
    /// Stable key used to return the submitted value.
    pub name: String,
    /// Prompt displayed above the input line.
    pub label: String,
    /// Whether an empty or whitespace-only value is rejected.
    pub required: bool,
    /// Value shown initially and returned when left unchanged.
    pub default: Option<String>,
    /// Short guidance shown after `Required`/`Optional`.
    pub hint: Option<String>,
    /// Validation applied to nonempty values before the form advances.
    pub check: Option<FieldCheck>,
}

impl TextField {
    /// Creates an optional field with no default.
    pub fn new(name: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            required: false,
            default: None,
            hint: None,
            check: None,
        }
    }

    /// Rejects empty values.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Sets the initial value.
    #[must_use]
    pub fn default_value(mut self, value: Option<String>) -> Self {
        self.default = value;
        self
    }

    /// Adds short guidance shown under the label.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Validates nonempty values before the form advances.
    #[must_use]
    pub fn check(mut self, check: impl Fn(&str) -> Result<(), String> + 'static) -> Self {
        self.check = Some(Rc::new(check));
        self
    }
}

impl fmt::Debug for TextField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextField")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("required", &self.required)
            .field("default", &self.default)
            .field("hint", &self.hint)
            .field("check", &self.check.is_some())
            .finish()
    }
}

/// Result of submitting, backing out of, or quitting an inline form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormOutcome {
    /// Submitted values indexed by each field's `name`.
    Submitted(BTreeMap<String, String>),
    /// The user pressed Esc to return to the previous screen without submitting.
    Back,
    /// The user requested to quit the interactive command.
    Quit,
}

/// Collects values in a compact inline form using the shared terminal UI.
///
/// Enter or Tab advances through fields. Shift+Tab returns to the previous field, Esc goes back,
/// and Ctrl+C quits. Unmodified letters, including `q`, remain available as input. Multi-field
/// forms show the step and the other fields below the active input.
///
/// # Errors
/// Returns an I/O error if the terminal cannot be initialized, drawn, or read. Duplicate field
/// names are rejected before the terminal is initialized.
pub fn run_form(heading: &Heading, fields: Vec<TextField>) -> io::Result<FormOutcome> {
    run_form_with_mode(heading, fields, false)
}

/// Collects one secret value without echoing its contents in the terminal.
///
/// Enter submits the value, Esc goes back, and Ctrl+C quits.
///
/// # Errors
/// Returns an I/O error if the terminal cannot be initialized, drawn, or read.
pub fn run_secret_input(heading: &Heading, field: TextField) -> io::Result<FormOutcome> {
    run_form_with_mode(heading, vec![field.required()], true)
}

fn run_form_with_mode(
    heading: &Heading,
    fields: Vec<TextField>,
    secret: bool,
) -> io::Result<FormOutcome> {
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

    let mut terminal = terminal::InlineTerminal::new(form_height(heading, fields.len()))?;
    let mut app = FormApp::new_with_mode(heading, fields, secret);
    loop {
        terminal.draw(|frame| draw_form(frame, &app))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = app.handle(key) {
                    terminal.close()?;
                    return Ok(outcome);
                }
            }
            _ => {}
        }
    }
}

fn form_height(heading: &Heading, fields: usize) -> u16 {
    // Heading and its gap, one row per field plus the active field's guidance, a gap, the footer.
    let fields = u16::try_from(fields).unwrap_or(u16::MAX);
    2 * heading.height() + heading.outline_height() + fields.saturating_add(1) + 2
}

/// Asks a yes/no question in a compact inline Ratatui screen.
///
/// Arrow keys or `h`/`l` switch the choice. Enter confirms, Esc goes back, and `q` or Ctrl+C quits.
///
/// # Errors
/// Returns an I/O error if the terminal cannot be initialized, drawn, or read.
pub fn run_confirmation(
    heading: &Heading,
    question: &str,
    default_yes: bool,
) -> io::Result<InteractionOutcome<bool>> {
    let height = CONFIRM_FIXED_ROWS + 2 * heading.height();
    let mut terminal = terminal::InlineTerminal::new(height)?;
    let mut yes = default_yes;
    loop {
        terminal.draw(|frame| draw_confirmation(frame, heading, question, yes))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc => {
                    terminal.close()?;
                    return Ok(InteractionOutcome::Back);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    terminal.close()?;
                    return Ok(InteractionOutcome::Quit);
                }
                KeyCode::Char('q') if key.modifiers.is_empty() => {
                    terminal.close()?;
                    return Ok(InteractionOutcome::Quit);
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
                    terminal.close()?;
                    return Ok(InteractionOutcome::Selected(yes));
                }
                _ => {}
            },
            _ => {}
        }
    }
}

struct FormApp {
    heading: Heading,
    fields: Vec<TextField>,
    secret: bool,
    values: BTreeMap<String, String>,
    index: usize,
    input: Vec<char>,
    cursor: usize,
    error: Option<String>,
}

impl FormApp {
    #[cfg(test)]
    fn new(heading: &Heading, fields: Vec<TextField>) -> Self {
        Self::new_with_mode(heading, fields, false)
    }

    fn new_with_mode(heading: &Heading, fields: Vec<TextField>, secret: bool) -> Self {
        let input = fields[0]
            .default
            .clone()
            .unwrap_or_default()
            .chars()
            .collect::<Vec<_>>();
        let cursor = input.len();
        Self {
            heading: heading.clone(),
            fields,
            secret,
            values: BTreeMap::new(),
            index: 0,
            cursor,
            input,
            error: None,
        }
    }

    fn handle(&mut self, key: KeyEvent) -> Option<FormOutcome> {
        if key.code == KeyCode::Esc {
            return Some(FormOutcome::Back);
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(FormOutcome::Quit);
        }

        match key.code {
            KeyCode::BackTab => return self.move_field(false),
            KeyCode::Tab | KeyCode::Enter => return self.move_field(true),
            _ => {}
        }
        if line_edit::apply(&mut self.input, &mut self.cursor, key) == line_edit::Edit::Changed {
            self.error = None;
        }

        None
    }

    fn move_field(&mut self, forward: bool) -> Option<FormOutcome> {
        let field = &self.fields[self.index];
        let value = if self.input.is_empty() {
            field.default.clone().unwrap_or_default()
        } else {
            self.input_text()
        };

        if forward && field.required && value.trim().is_empty() {
            self.error = Some("A value is required.".to_owned());
            return None;
        }

        if forward
            && !value.trim().is_empty()
            && let Some(check) = &field.check
            && let Err(error) = check(value.trim())
        {
            self.error = Some(error);
            return None;
        }

        self.values
            .insert(field.name.clone(), value.trim().to_owned());
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
            .unwrap_or_default()
            .chars()
            .collect();
        self.cursor = self.input.len();
        None
    }

    fn input_text(&self) -> String {
        self.input.iter().collect()
    }
}

fn draw_form(frame: &mut Frame<'_>, app: &FormApp) {
    let heading_height = app.heading.height();
    let rows = u16::try_from(app.fields.len() + 1).unwrap_or(u16::MAX);
    let [
        heading_area,
        _heading_gap,
        outline_area,
        fields_area,
        _footer_gap,
        footer_area,
    ] = Layout::vertical([
        Constraint::Length(heading_height),
        Constraint::Length(heading_height),
        Constraint::Length(app.heading.outline_height()),
        Constraint::Length(rows),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let step =
        (app.fields.len() > 1).then(|| format!("step {} of {}", app.index + 1, app.fields.len()));
    frame.render_widget(
        Paragraph::new(app.heading.line(step.as_deref(), heading_area.width)),
        heading_area,
    );

    if app.heading.outline_height() > 0 {
        frame.render_widget(Paragraph::new(app.heading.outline_lines()), outline_area);
    }

    let label_width = label_column_width(app);
    let value_column = FIELD_MARKER_WIDTH + label_width + FIELD_GAP;
    let input_width = fields_area
        .width
        .saturating_sub(u16::try_from(value_column).unwrap_or(u16::MAX));
    let (lines, cursor) = field_rows(app, label_width, input_width);
    frame.render_widget(Paragraph::new(lines), fields_area);

    let last = app.index + 1 == app.fields.len();
    let hints = [
        essential("Enter", if last { "finish" } else { "next" }),
        hint("Shift+Tab", "previous"),
        essential("Esc", "back"),
    ];
    let hints = if app.index == 0 {
        vec![hints[0], hints[2]]
    } else {
        hints.to_vec()
    };
    frame.render_widget(
        Paragraph::new(footer(&hints, footer_area.width)),
        footer_area,
    );

    let x = fields_area
        .x
        .saturating_add(u16::try_from(value_column).unwrap_or(u16::MAX))
        .saturating_add(cursor.1);
    let y = fields_area.y.saturating_add(cursor.0);
    if x < fields_area.right() {
        frame.set_cursor_position((x, y));
    }
}

/// Columns before a field label: the state marker and a space.
const FIELD_MARKER_WIDTH: usize = 2;
/// Columns between the longest label and the value column.
const FIELD_GAP: usize = 3;

fn label_column_width(app: &FormApp) -> usize {
    app.fields
        .iter()
        .map(|field| UnicodeWidthStr::width(terminal_text(&field.label).as_str()))
        .max()
        .unwrap_or_default()
}

/// Every field in order, so the list never reshuffles: finished fields show a check and their
/// value, the active field becomes the input with its guidance underneath, and upcoming fields
/// wait muted. Returns the lines and the cursor's (row, column) within the value column.
fn field_rows(
    app: &FormApp,
    label_width: usize,
    input_width: u16,
) -> (Vec<Line<'static>>, (u16, u16)) {
    let mut lines = Vec::new();
    let mut cursor = (0, 0);
    for (index, field) in app.fields.iter().enumerate() {
        let label = terminal_text(&field.label);
        let padding = " ".repeat(
            (label_width + FIELD_GAP).saturating_sub(UnicodeWidthStr::width(label.as_str())),
        );
        if index == app.index {
            let (shown, column) = if app.secret {
                visible_input(&"•".repeat(app.input.len()), app.cursor, input_width)
            } else {
                visible_input(&app.input_text(), app.cursor, input_width)
            };
            let value = if app.input.is_empty() && !app.secret {
                Span::styled("Type a value…", tui_style(Tone::Muted, false))
            } else {
                Span::raw(shown)
            };
            cursor = (u16::try_from(lines.len()).unwrap_or(u16::MAX), column);
            lines.push(Line::from(vec![
                Span::styled("› ", tui_style(Tone::Accent, true)),
                Span::styled(label, tui_style(Tone::Accent, true)),
                Span::raw(padding),
                value,
            ]));
            lines.push(guidance_line(app, field));
        } else if let Some(value) = app.values.get(&field.name) {
            let value = if app.secret {
                "•".repeat(value.chars().count())
            } else if value.trim().is_empty() {
                "—".to_owned()
            } else {
                terminal_text(value)
            };
            lines.push(Line::from(vec![
                Span::styled("✓ ", tui_style(Tone::Success, false)),
                Span::raw(label),
                Span::raw(padding),
                Span::styled(value, tui_style(Tone::Secondary, false)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("· ", tui_style(Tone::Muted, false)),
                Span::styled(label, tui_style(Tone::Muted, false)),
            ]));
        }
    }
    (lines, cursor)
}

/// The line under the active field: a validation error, or Required/Optional with its hint.
fn guidance_line(app: &FormApp, field: &TextField) -> Line<'static> {
    if let Some(error) = &app.error {
        return Line::from(vec![
            Span::raw("  "),
            Span::styled(terminal_text(error), tui_style(Tone::Error, false)),
        ]);
    }
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(
            if field.required {
                "Required"
            } else {
                "Optional"
            },
            tui_style(Tone::Secondary, false),
        ),
    ];
    if let Some(hint) = &field.hint {
        spans.extend([
            Span::styled(" · ", tui_style(Tone::Muted, false)),
            Span::styled(terminal_text(hint), tui_style(Tone::Secondary, false)),
        ]);
    }
    Line::from(spans)
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

fn draw_confirmation(frame: &mut Frame<'_>, heading: &Heading, question: &str, yes: bool) {
    let heading_height = heading.height();
    let [
        heading_area,
        _heading_gap,
        question_area,
        _gap,
        choice_area,
        _footer_gap,
        footer_area,
    ] = Layout::vertical([
        Constraint::Length(heading_height),
        Constraint::Length(heading_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(
        Paragraph::new(heading.line(None, heading_area.width)),
        heading_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::raw(terminal_text(question)),
        ])),
        question_area,
    );
    let choice = |label: &'static str, active: bool| {
        if active {
            Span::styled(format!("[ {label} ]"), tui_style(Tone::Accent, true))
        } else {
            Span::styled(format!("  {label}  "), tui_style(Tone::Secondary, false))
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            choice("Yes", yes),
            Span::raw(" "),
            choice("No", !yes),
        ])),
        choice_area,
    );
    frame.render_widget(
        Paragraph::new(footer(
            &[
                essential("←/→", "choose"),
                hint("y/n", "answer"),
                essential("Enter", "confirm"),
                essential("Esc", "back"),
                essential("q", "quit"),
            ],
            footer_area.width,
        )),
        footer_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    fn field(name: &str, required: bool, default: Option<&str>) -> TextField {
        let field = TextField::new(name, name).default_value(default.map(str::to_owned));
        if required { field.required() } else { field }
    }

    fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        let width = usize::from(buffer.area.width);
        buffer
            .content
            .chunks(width)
            .map(|row| {
                row.iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn form_returns_submitted_values_by_field_name() {
        let mut app = FormApp::new(
            &Heading::new("Create template"),
            vec![
                field("category", true, Some("custom")),
                field("description", true, None),
            ],
        );
        assert_eq!(app.input_text(), "custom");
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
            &Heading::new("Create template"),
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
        let mut app = FormApp::new(
            &Heading::new("Create template"),
            vec![field("name", true, Some("café"))],
        );
        app.cursor = 3;
        app.handle(key(KeyCode::Backspace));
        app.handle(key(KeyCode::Char('o')));
        assert_eq!(app.input_text(), "caoé");
        app.handle(key(KeyCode::Home));
        app.handle(key(KeyCode::Delete));
        assert_eq!(app.input_text(), "aoé");
    }

    #[test]
    fn visible_input_keeps_cursor_in_narrow_terminal() {
        let (visible, cursor) = visible_input("some-long-template-id", 20, 8);
        assert!(visible.chars().count() <= 8);
        assert!(cursor < 8);
        assert!(visible.starts_with('…'));
    }

    #[test]
    fn secret_form_masks_the_value_but_returns_it_unchanged() {
        let secret = "private-key";
        let heading = Heading::new("save API key");
        let mut app = FormApp::new_with_mode(&heading, vec![field("secret", true, None)], true);
        app.input = secret.chars().collect();
        app.cursor = secret.chars().count();
        let backend = TestBackend::new(40, form_height(&heading, 1));
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw_form(frame, &app)).unwrap();
        let displayed = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!displayed.contains(secret));
        assert!(displayed.contains(&"•".repeat(secret.len())));
        assert_eq!(
            app.handle(key(KeyCode::Enter)),
            Some(FormOutcome::Submitted(BTreeMap::from([(
                "secret".to_owned(),
                secret.to_owned(),
            )])))
        );
    }

    #[test]
    fn form_rejects_required_empty_values_and_cancels_without_submission() {
        let mut app = FormApp::new(
            &Heading::new("Create template"),
            vec![field("description", true, None)],
        );
        assert_eq!(app.handle(key(KeyCode::Enter)), None);
        assert_eq!(app.error.as_deref(), Some("A value is required."));
        assert_eq!(app.handle(key(KeyCode::Esc)), Some(FormOutcome::Back));
    }

    #[test]
    fn ctrl_c_quits_while_escape_returns_from_a_form() {
        let mut app = FormApp::new(
            &Heading::new("Profile"),
            vec![field("Profile ID", true, None)],
        );

        assert_eq!(
            app.handle(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL,)),
            Some(FormOutcome::Quit)
        );
        assert_eq!(app.handle(key(KeyCode::Esc)), Some(FormOutcome::Back));
    }

    #[test]
    fn form_shows_heading_step_field_and_overview_of_other_fields() {
        let heading = Heading::new("new profile").context("OpenAI");
        let mut app = FormApp::new(
            &heading,
            vec![
                field("Profile ID", true, None),
                field("Model ID", true, Some("gpt-5")),
            ],
        );
        for character in "work".chars() {
            app.handle(key(KeyCode::Char(character)));
        }
        app.handle(key(KeyCode::Enter));
        let mut terminal = Terminal::new(TestBackend::new(72, form_height(&heading, 2))).unwrap();
        terminal.draw(|frame| draw_form(frame, &app)).unwrap();
        let rows = rows(&terminal);

        assert!(rows[0].starts_with("Inifra · new profile · OpenAI"));
        assert!(rows[0].ends_with("step 2 of 2"));
        assert_eq!(rows[1], "");
        // Every field keeps its place: finished above, the active one editing in place.
        assert_eq!(rows[2], "✓ Profile ID   work");
        assert_eq!(rows[3], "› Model ID     gpt-5");
        assert_eq!(rows[4], "  Required");
        assert_eq!(rows[5], "");
        assert!(rows[6].contains("Enter finish"));
        assert!(rows[6].contains("Shift+Tab previous"));
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(2, 3)].fg, tui_style(Tone::Accent, true).fg.unwrap());
        assert_eq!(
            buffer[(0, 2)].fg,
            tui_style(Tone::Success, false).fg.unwrap()
        );
    }

    #[test]
    fn first_field_lists_upcoming_fields_and_omits_previous_hint() {
        let heading = Heading::new("new profile");
        let app = FormApp::new(
            &heading,
            vec![
                field("Profile ID", true, None).hint("lowercase letters, numbers, hyphens"),
                field("Model ID", true, None),
            ],
        );
        let mut terminal = Terminal::new(TestBackend::new(72, form_height(&heading, 2))).unwrap();
        terminal.draw(|frame| draw_form(frame, &app)).unwrap();
        let rows = rows(&terminal);

        assert_eq!(rows[2], "› Profile ID   Type a value…");
        assert_eq!(rows[3], "  Required · lowercase letters, numbers, hyphens");
        assert_eq!(rows[4], "· Model ID");
        assert!(!rows[6].contains("previous"));
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(2, 4)].fg, tui_style(Tone::Muted, false).fg.unwrap());
    }

    #[test]
    fn the_cursor_sits_in_the_active_row_after_the_typed_text() {
        let heading = Heading::new("new profile");
        let mut app = FormApp::new(
            &heading,
            vec![field("Name", true, None), field("Model", true, None)],
        );
        for character in "ab".chars() {
            app.handle(key(KeyCode::Char(character)));
        }
        let mut terminal = Terminal::new(TestBackend::new(60, form_height(&heading, 2))).unwrap();
        terminal.draw(|frame| draw_form(frame, &app)).unwrap();
        // "› " + "Name" label column (5) + 3 gap + two typed characters.
        terminal
            .backend_mut()
            .assert_cursor_position((2 + 5 + 3 + 2, 2));
    }

    #[test]
    fn submitted_values_drop_surrounding_whitespace() {
        let mut app = FormApp::new(&Heading::none(), vec![field("Model", true, None)]);
        for character in "  gpt-5 ".chars() {
            app.handle(key(KeyCode::Char(character)));
        }
        let Some(FormOutcome::Submitted(values)) = app.handle(key(KeyCode::Enter)) else {
            panic!("the form should submit");
        };
        assert_eq!(values["Model"], "gpt-5");
    }

    #[test]
    fn word_editing_keys_work_in_a_field() {
        let mut app = FormApp::new(&Heading::none(), vec![field("Name", true, None)]);
        for character in "alpha beta".chars() {
            app.handle(key(KeyCode::Char(character)));
        }
        app.handle(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
        assert_eq!(app.cursor, 6);
        app.handle(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(app.input_text(), "beta");
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn field_checks_reject_values_before_advancing() {
        fn lowercase(value: &str) -> Result<(), String> {
            if value
                .chars()
                .all(|character| character.is_ascii_lowercase())
            {
                Ok(())
            } else {
                Err("Use lowercase letters.".to_owned())
            }
        }

        let heading = Heading::none();
        let mut app = FormApp::new(
            &heading,
            vec![field("id", true, Some("Work")).check(lowercase)],
        );
        assert_eq!(app.handle(key(KeyCode::Enter)), None);
        assert_eq!(app.error.as_deref(), Some("Use lowercase letters."));
        app.handle(key(KeyCode::Home));
        app.handle(key(KeyCode::Delete));
        app.handle(key(KeyCode::Char('w')));
        assert_eq!(
            app.handle(key(KeyCode::Enter)),
            Some(FormOutcome::Submitted(BTreeMap::from([(
                "id".to_owned(),
                "work".to_owned()
            )])))
        );
    }

    #[test]
    fn confirmation_draws_without_layout_overflow() {
        let heading = Heading::new("create template");
        let backend = TestBackend::new(80, CONFIRM_FIXED_ROWS + 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_confirmation(frame, &heading, "Create 'example'?", false))
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
