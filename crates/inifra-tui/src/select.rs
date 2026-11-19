use crate::{
    presentation::{Tone, terminal_text, tui_style},
    terminal,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};
use std::io;

const MAX_VISIBLE_CHOICES: usize = 8;

/// Shows a compact inline choice list and returns the selected row.
///
/// Arrow keys or `j`/`k` move through the list; Enter selects and Esc cancels.
///
/// # Errors
/// Returns an I/O error if the terminal cannot be initialized, drawn, or read. Empty choice lists
/// are rejected before the terminal is initialized.
pub fn run_select(title: &str, choices: &[String], initial: usize) -> io::Result<Option<usize>> {
    if choices.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a choice list must contain at least one option",
        ));
    }

    let title_height = u16::from(!title.is_empty());
    let visible = choices.len().min(MAX_VISIBLE_CHOICES) as u16;
    let (_restore, mut terminal) = terminal::inline(title_height + visible + 1)?;
    let mut app = SelectApp::new(choices, initial);
    loop {
        terminal.draw(|frame| draw_select(frame, title, choices, &mut app))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = app.handle(key, choices.len()) {
                    let _ = terminal.clear();
                    return Ok(match outcome {
                        SelectOutcome::Selected(index) => Some(index),
                        SelectOutcome::Cancelled => None,
                    });
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectOutcome {
    Selected(usize),
    Cancelled,
}

struct SelectApp {
    state: ListState,
}

impl SelectApp {
    fn new(choices: &[String], initial: usize) -> Self {
        let selected = initial.min(choices.len() - 1);
        Self {
            state: ListState::default().with_selected(Some(selected)),
        }
    }

    fn selected(&self) -> usize {
        self.state.selected().unwrap_or_default()
    }

    fn handle(&mut self, key: KeyEvent, len: usize) -> Option<SelectOutcome> {
        match key.code {
            KeyCode::Esc => Some(SelectOutcome::Cancelled),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(SelectOutcome::Cancelled)
            }
            KeyCode::Enter => Some(SelectOutcome::Selected(self.selected())),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('h') => {
                self.move_by(-1, len);
                None
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('l') => {
                self.move_by(1, len);
                None
            }
            KeyCode::Home => {
                self.state.select(Some(0));
                None
            }
            KeyCode::End => {
                self.state.select(Some(len - 1));
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
        self.state.select(Some(next));
    }
}

fn draw_select(frame: &mut Frame<'_>, title: &str, choices: &[String], app: &mut SelectApp) {
    let area = frame.area();
    let title_height = u16::from(!title.is_empty());
    let [title_area, choices_area, footer_area] = Layout::vertical([
        Constraint::Length(title_height),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    if !title.is_empty() {
        frame.render_widget(
            Paragraph::new(terminal_text(title)).style(tui_style(Tone::Accent, true)),
            title_area,
        );
    }

    let items = choices
        .iter()
        .map(|choice| {
            ListItem::new(Line::from(Span::raw(terminal_text(choice))))
                .style(tui_style(Tone::Muted, false))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .highlight_style(tui_style(Tone::Accent, true))
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, choices_area, &mut app.state);

    let footer = Line::from(vec![
        Span::styled("↑/↓ j/k", tui_style(Tone::Accent, true)),
        Span::raw(" move  ·  "),
        Span::styled("Enter", tui_style(Tone::Accent, true)),
        Span::raw(" select  ·  "),
        Span::styled("Esc", tui_style(Tone::Accent, true)),
        Span::raw(" back"),
    ]);
    frame.render_widget(Paragraph::new(footer), footer_area);
}

#[cfg(test)]
mod tests {
    use super::{SelectApp, SelectOutcome};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn arrow_and_vim_keys_move_and_selection_is_clamped() {
        let choices = ["one".to_owned(), "two".to_owned(), "three".to_owned()];
        let mut app = SelectApp::new(&choices, 0);

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
        let mut app = SelectApp::new(&choices, 1);

        assert_eq!(
            app.handle(key(KeyCode::Enter), choices.len()),
            Some(SelectOutcome::Selected(1))
        );
        assert_eq!(
            app.handle(key(KeyCode::Esc), choices.len()),
            Some(SelectOutcome::Cancelled)
        );
    }
}
