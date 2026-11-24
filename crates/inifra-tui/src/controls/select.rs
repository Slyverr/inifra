use super::{
    InteractionOutcome,
    chrome::{Heading, Row, RowState, essential, footer, label_width, row_line},
};
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
use std::io;

const MAX_VISIBLE_CHOICES: usize = 8;

/// Content of an inline choice screen.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectScreen {
    /// Heading row; [`Heading::none`] omits it.
    pub heading: Heading,
    /// Read-only rows shown above the choices, such as saved profiles.
    pub summary: Vec<Row>,
    /// A secondary line shown below the summary, such as an empty-state message.
    pub note: Option<String>,
    /// Selectable rows.
    pub choices: Vec<Row>,
    /// Initially focused choice; clamped to the list.
    pub initial: usize,
}

impl SelectScreen {
    /// Creates a choice screen with a heading and plain choice labels.
    pub fn new(heading: Heading, choices: impl IntoIterator<Item = impl Into<Row>>) -> Self {
        Self {
            heading,
            choices: choices.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    fn context_height(&self) -> u16 {
        let rows = self.summary.len() + usize::from(self.note.is_some());
        if rows == 0 {
            0
        } else {
            u16::try_from(rows + 1).unwrap_or(u16::MAX)
        }
    }

    fn heading_height(&self) -> u16 {
        self.heading.height() * 2
    }
}

/// Shows a compact inline choice list and returns the selected row.
///
/// Arrow keys or `j`/`k` move through the list; Enter selects, Esc goes back, and `q` or Ctrl+C
/// quits. The focused row is marked with `›` so focus stays visible without color.
///
/// # Errors
/// Returns an I/O error if the terminal cannot be initialized, drawn, or read. Empty choice lists
/// are rejected before the terminal is initialized.
pub fn run_select(screen: &SelectScreen) -> io::Result<InteractionOutcome<usize>> {
    let choices = &screen.choices;
    if choices.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a choice list must contain at least one option",
        ));
    }

    let visible = choices.len().min(MAX_VISIBLE_CHOICES) as u16;
    let height = screen.heading_height() + screen.context_height() + visible + 2;
    let mut terminal = terminal::InlineTerminal::new(height)?;
    let mut app = SelectApp::new(choices.len(), screen.initial);
    loop {
        terminal.draw(|frame| draw_select(frame, screen, &app))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = app.handle(key, choices.len()) {
                    terminal.close()?;
                    return Ok(outcome);
                }
            }
            _ => {}
        }
    }
}

struct SelectApp {
    selected: usize,
}

impl SelectApp {
    fn new(len: usize, initial: usize) -> Self {
        Self {
            selected: initial.min(len.saturating_sub(1)),
        }
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn handle(&mut self, key: KeyEvent, len: usize) -> Option<InteractionOutcome<usize>> {
        match key.code {
            KeyCode::Esc => Some(InteractionOutcome::Back),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(InteractionOutcome::Quit)
            }
            KeyCode::Char('q') if key.modifiers.is_empty() => Some(InteractionOutcome::Quit),
            KeyCode::Enter => Some(InteractionOutcome::Selected(self.selected())),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('h') => {
                self.move_by(-1, len);
                None
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('l') => {
                self.move_by(1, len);
                None
            }
            KeyCode::Home => {
                self.selected = 0;
                None
            }
            KeyCode::End => {
                self.selected = len - 1;
                None
            }
            KeyCode::PageUp => {
                self.move_by(-(MAX_VISIBLE_CHOICES as isize), len);
                None
            }
            KeyCode::PageDown => {
                self.move_by(MAX_VISIBLE_CHOICES as isize, len);
                None
            }
            _ => None,
        }
    }

    fn move_by(&mut self, amount: isize, len: usize) {
        let next = (self.selected() as isize + amount).clamp(0, len as isize - 1) as usize;
        self.selected = next;
    }
}

fn draw_select(frame: &mut Frame<'_>, screen: &SelectScreen, app: &SelectApp) {
    let area = frame.area();
    let [
        heading_area,
        context_area,
        choices_area,
        _footer_gap,
        footer_area,
    ] = Layout::vertical([
        Constraint::Length(screen.heading_height()),
        Constraint::Length(screen.context_height()),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(screen.heading.line(None, heading_area.width)),
        heading_area,
    );

    if screen.context_height() > 0 {
        let width = label_width(&screen.summary);
        let mut lines = screen
            .summary
            .iter()
            .map(|row| row_line(row, RowState::Summary, width))
            .collect::<Vec<_>>();
        if let Some(note) = &screen.note {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(terminal_text(note), tui_style(Tone::Secondary, false)),
            ]));
        }
        frame.render_widget(Paragraph::new(lines), context_area);
    }

    let count = usize::from(choices_area.height).max(1);
    let start = app
        .selected()
        .saturating_sub(count.saturating_sub(1))
        .min(screen.choices.len().saturating_sub(count));
    let width = label_width(&screen.choices);
    let lines = screen
        .choices
        .iter()
        .enumerate()
        .skip(start)
        .take(count)
        .map(|(index, row)| {
            let state = if index == app.selected() {
                RowState::Focused
            } else {
                RowState::Idle
            };
            row_line(row, state, width)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), choices_area);

    frame.render_widget(
        Paragraph::new(footer(
            &[
                essential("↑/↓ j/k", "move"),
                essential("Enter", "select"),
                essential("Esc", "back"),
                essential("q/Ctrl+C", "quit"),
            ],
            footer_area.width,
        )),
        footer_area,
    );
}

#[cfg(test)]
mod tests {
    use super::{InteractionOutcome, SelectApp, SelectScreen, draw_select};
    use crate::controls::chrome::{Badge, Heading, Row};
    use crate::presentation::Tone;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn arrow_and_vim_keys_move_and_selection_is_clamped() {
        let choices = ["one".to_owned(), "two".to_owned(), "three".to_owned()];
        let mut app = SelectApp::new(choices.len(), 0);

        app.handle(key(KeyCode::Char('j')), choices.len());
        assert_eq!(app.selected(), 1);
        app.handle(key(KeyCode::Down), choices.len());
        assert_eq!(app.selected(), 2);
        app.handle(key(KeyCode::Right), choices.len());
        assert_eq!(app.selected(), 2);
        app.handle(key(KeyCode::Char('k')), choices.len());
        assert_eq!(app.selected(), 1);
        app.handle(key(KeyCode::Left), choices.len());
        assert_eq!(app.selected(), 0);
    }

    #[test]
    fn enter_selects_and_escape_cancels() {
        let choices = ["one".to_owned(), "two".to_owned()];
        let mut app = SelectApp::new(choices.len(), 1);

        assert_eq!(
            app.handle(key(KeyCode::Enter), choices.len()),
            Some(InteractionOutcome::Selected(1))
        );
        assert_eq!(
            app.handle(key(KeyCode::Esc), choices.len()),
            Some(InteractionOutcome::Back)
        );
    }

    #[test]
    fn q_and_ctrl_c_quit_without_becoming_back_navigation() {
        let choices = ["one".to_owned(), "two".to_owned()];
        let mut app = SelectApp::new(choices.len(), 0);

        assert_eq!(
            app.handle(key(KeyCode::Char('q')), choices.len()),
            Some(InteractionOutcome::Quit)
        );
        assert_eq!(
            app.handle(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                choices.len()
            ),
            Some(InteractionOutcome::Quit)
        );
    }

    #[test]
    fn empty_choice_list_fails_before_terminal_initialization() {
        let error = super::run_select(&SelectScreen::default()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    fn render(screen: &SelectScreen, app: &SelectApp, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| draw_select(frame, screen, app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..usize::from(height))
            .map(|row| {
                buffer.content[row * usize::from(width)..(row + 1) * usize::from(width)]
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn selection_screen_marks_focus_with_a_glyph_and_aligns_summary_rows() {
        let mut screen = SelectScreen::new(
            Heading::new("assistant profiles"),
            ["Create or edit a profile", "Reset assistant data"],
        );
        screen.summary = vec![
            Row::new("local")
                .detail("Ollama · qwen3:8b")
                .badge(Badge::new("default", Tone::Success)),
            Row::new("work").detail("OpenAI · gpt-5"),
        ];
        let app = SelectApp::new(screen.choices.len(), 1);
        let rows = render(&screen, &app, 60, 9);

        assert_eq!(rows[0], "Inifra · assistant profiles");
        assert_eq!(rows[1], "");
        assert_eq!(rows[2], "  local   Ollama · qwen3:8b   default");
        assert_eq!(rows[3], "  work    OpenAI · gpt-5");
        assert_eq!(rows[4], "");
        assert_eq!(rows[5], "  Create or edit a profile");
        assert_eq!(rows[6], "› Reset assistant data");
        assert!(rows[8].contains("Enter select"));
        assert!(rows[8].contains("Esc back"));
    }

    #[test]
    fn selection_screen_without_heading_or_summary_starts_with_choices() {
        let screen = SelectScreen::new(Heading::none(), ["one", "two"]);
        let app = SelectApp::new(screen.choices.len(), 0);
        let rows = render(&screen, &app, 50, 4);
        assert_eq!(rows[0], "› one");
        assert_eq!(rows[1], "  two");
    }

    #[test]
    fn long_lists_keep_the_focused_choice_visible() {
        let screen = SelectScreen::new(
            Heading::none(),
            (0..12).map(|index| format!("choice {index}")),
        );
        let app = SelectApp::new(screen.choices.len(), 11);
        let rows = render(&screen, &app, 40, 10);
        assert!(rows.iter().any(|row| row == "› choice 11"));
    }
}
