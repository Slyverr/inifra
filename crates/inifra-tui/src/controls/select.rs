use super::{
    InteractionOutcome,
    chrome::{Heading, Row, RowState, essential, footer, label_width, row_line},
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
use std::io;
use unicode_width::UnicodeWidthStr;

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
    /// Whether `/` starts filtering the choices by what the user types. Use it for long lists.
    pub filterable: bool,
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
    let mut app = SelectApp::new(screen);
    loop {
        terminal.draw(|frame| draw_select(frame, screen, &app))?;
        match event::read()? {
            Event::Resize(_, _) => terminal.autoresize()?,
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(outcome) = app.handle(key, choices) {
                    terminal.close()?;
                    return Ok(outcome);
                }
            }
            _ => {}
        }
    }
}

struct SelectApp {
    /// Position within `visible`.
    selected: usize,
    /// Indexes into the screen's choices that match the filter, in order.
    visible: Vec<usize>,
    filterable: bool,
    typing: bool,
    filter: Vec<char>,
    filter_cursor: usize,
}

impl SelectApp {
    fn new(screen: &SelectScreen) -> Self {
        let visible = (0..screen.choices.len()).collect::<Vec<_>>();
        Self {
            selected: screen.initial.min(visible.len().saturating_sub(1)),
            visible,
            filterable: screen.filterable,
            typing: false,
            filter: Vec::new(),
            filter_cursor: 0,
        }
    }

    /// The focused position within the visible rows.
    fn selected(&self) -> usize {
        self.selected
    }

    /// The index of the focused row in the screen's choices, if any row is visible.
    fn focused_choice(&self) -> Option<usize> {
        self.visible.get(self.selected).copied()
    }

    fn filter_text(&self) -> String {
        self.filter.iter().collect()
    }

    fn handle(&mut self, key: KeyEvent, choices: &[Row]) -> Option<InteractionOutcome<usize>> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Char('c') && control {
            return Some(InteractionOutcome::Quit);
        }
        if self.typing {
            return self.handle_filter(key, choices);
        }
        match key.code {
            KeyCode::Esc => Some(InteractionOutcome::Back),
            KeyCode::Char('q') if key.modifiers.is_empty() => Some(InteractionOutcome::Quit),
            KeyCode::Char('/') if self.filterable && key.modifiers.is_empty() => {
                self.typing = true;
                None
            }
            KeyCode::Enter => self.focused_choice().map(InteractionOutcome::Selected),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('h') => {
                self.move_by(-1);
                None
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('l') => {
                self.move_by(1);
                None
            }
            KeyCode::Home => {
                self.selected = 0;
                None
            }
            KeyCode::End => {
                self.selected = self.visible.len().saturating_sub(1);
                None
            }
            KeyCode::PageUp => {
                self.move_by(-(MAX_VISIBLE_CHOICES as isize));
                None
            }
            KeyCode::PageDown => {
                self.move_by(MAX_VISIBLE_CHOICES as isize);
                None
            }
            _ => None,
        }
    }

    /// While filtering, letters edit the filter; arrows still move and Enter still selects.
    fn handle_filter(
        &mut self,
        key: KeyEvent,
        choices: &[Row],
    ) -> Option<InteractionOutcome<usize>> {
        match key.code {
            KeyCode::Esc => {
                self.typing = false;
                self.filter.clear();
                self.filter_cursor = 0;
                self.refilter(choices);
            }
            KeyCode::Enter => return self.focused_choice().map(InteractionOutcome::Selected),
            KeyCode::Up => self.move_by(-1),
            KeyCode::Down => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(MAX_VISIBLE_CHOICES as isize)),
            KeyCode::PageDown => self.move_by(MAX_VISIBLE_CHOICES as isize),
            KeyCode::Backspace if self.filter.is_empty() => self.typing = false,
            _ => {
                if line_edit::apply(&mut self.filter, &mut self.filter_cursor, key)
                    == line_edit::Edit::Changed
                {
                    self.refilter(choices);
                }
            }
        }
        None
    }

    /// Keeps the focused row when it still matches; otherwise focuses the first match.
    fn refilter(&mut self, choices: &[Row]) {
        let focused = self.focused_choice();
        let needle = self.filter_text().to_lowercase();
        self.visible = choices
            .iter()
            .enumerate()
            .filter(|(_, row)| needle.is_empty() || row.label.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();
        self.selected = focused
            .and_then(|focused| self.visible.iter().position(|index| *index == focused))
            .unwrap_or_default();
    }

    fn move_by(&mut self, amount: isize) {
        let last = self.visible.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + amount).clamp(0, last) as usize;
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
        .min(app.visible.len().saturating_sub(count));
    let width = label_width(&screen.choices);
    let mut lines = app
        .visible
        .iter()
        .enumerate()
        .skip(start)
        .take(count)
        .map(|(position, index)| {
            let state = if position == app.selected() {
                RowState::Focused
            } else {
                RowState::Idle
            };
            row_line(&screen.choices[*index], state, width)
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("No matches", tui_style(Tone::Secondary, false)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), choices_area);

    if app.typing {
        let text = terminal_text(&app.filter_text());
        let prefix = "/ ";
        let used = prefix.len() + UnicodeWidthStr::width(text.as_str()) + 3;
        let hints = footer(
            &[essential("Enter", "select"), essential("Esc", "clear")],
            footer_area
                .width
                .saturating_sub(u16::try_from(used).unwrap_or(u16::MAX)),
        );
        let mut spans = vec![
            Span::styled(prefix, tui_style(Tone::Accent, true)),
            Span::raw(text),
            Span::raw("   "),
        ];
        spans.extend(hints.spans);
        frame.render_widget(Paragraph::new(Line::from(spans)), footer_area);
        let before = app.filter[..app.filter_cursor].iter().collect::<String>();
        let x = footer_area.x.saturating_add(
            u16::try_from(prefix.len() + UnicodeWidthStr::width(before.as_str()))
                .unwrap_or(u16::MAX),
        );
        if x < footer_area.right() {
            frame.set_cursor_position((x, footer_area.y));
        }
        return;
    }

    let mut hints = vec![
        essential("↑/↓ j/k", "move"),
        essential("Enter", "select"),
        essential("Esc", "back"),
        essential("q/Ctrl+C", "quit"),
    ];
    if screen.filterable {
        hints.insert(1, essential("/", "filter"));
    }
    frame.render_widget(
        Paragraph::new(footer(&hints, footer_area.width)),
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

    fn screen_of(labels: &[&str], initial: usize) -> SelectScreen {
        SelectScreen {
            choices: labels.iter().map(|label| Row::new(*label)).collect(),
            initial,
            ..SelectScreen::default()
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn arrow_and_vim_keys_move_and_selection_is_clamped() {
        let screen = screen_of(&["one", "two", "three"], 0);
        let choices = &screen.choices;
        let mut app = SelectApp::new(&screen);

        app.handle(key(KeyCode::Char('j')), choices);
        assert_eq!(app.selected(), 1);
        app.handle(key(KeyCode::Down), choices);
        assert_eq!(app.selected(), 2);
        app.handle(key(KeyCode::Right), choices);
        assert_eq!(app.selected(), 2);
        app.handle(key(KeyCode::Char('k')), choices);
        assert_eq!(app.selected(), 1);
        app.handle(key(KeyCode::Left), choices);
        assert_eq!(app.selected(), 0);
    }

    #[test]
    fn enter_selects_and_escape_cancels() {
        let screen = screen_of(&["one", "two"], 1);
        let choices = &screen.choices;
        let mut app = SelectApp::new(&screen);

        assert_eq!(
            app.handle(key(KeyCode::Enter), choices),
            Some(InteractionOutcome::Selected(1))
        );
        assert_eq!(
            app.handle(key(KeyCode::Esc), choices),
            Some(InteractionOutcome::Back)
        );
    }

    #[test]
    fn q_and_ctrl_c_quit_without_becoming_back_navigation() {
        let screen = screen_of(&["one", "two"], 0);
        let choices = &screen.choices;
        let mut app = SelectApp::new(&screen);

        assert_eq!(
            app.handle(key(KeyCode::Char('q')), choices),
            Some(InteractionOutcome::Quit)
        );
        assert_eq!(
            app.handle(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                choices
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
        screen.initial = 1;
        let app = SelectApp::new(&screen);
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
        let app = SelectApp::new(&screen);
        let rows = render(&screen, &app, 50, 4);
        assert_eq!(rows[0], "› one");
        assert_eq!(rows[1], "  two");
    }

    #[test]
    fn long_lists_keep_the_focused_choice_visible() {
        let mut screen = SelectScreen::new(
            Heading::none(),
            (0..12).map(|index| format!("choice {index}")),
        );
        screen.initial = 11;
        let app = SelectApp::new(&screen);
        let rows = render(&screen, &app, 40, 10);
        assert!(rows.iter().any(|row| row == "› choice 11"));
    }

    fn models() -> SelectScreen {
        SelectScreen {
            heading: Heading::none(),
            choices: ["gpt-5", "gpt-5-mini", "o4-mini", "text-embedding"]
                .into_iter()
                .map(Row::new)
                .collect(),
            filterable: true,
            ..SelectScreen::default()
        }
    }

    fn type_text(app: &mut SelectApp, choices: &[Row], text: &str) {
        for character in text.chars() {
            app.handle(key(KeyCode::Char(character)), choices);
        }
    }

    #[test]
    fn slash_filters_long_lists_and_enter_returns_the_original_index() {
        let screen = models();
        let mut app = SelectApp::new(&screen);
        // `j`, `k`, and `q` are ordinary text once filtering starts.
        assert_eq!(app.handle(key(KeyCode::Char('/')), &screen.choices), None);
        type_text(&mut app, &screen.choices, "mini");
        assert_eq!(app.visible, [1, 2]);
        app.handle(key(KeyCode::Down), &screen.choices);
        assert_eq!(
            app.handle(key(KeyCode::Enter), &screen.choices),
            Some(InteractionOutcome::Selected(2))
        );
    }

    #[test]
    fn escape_clears_the_filter_before_it_goes_back() {
        let screen = models();
        let mut app = SelectApp::new(&screen);
        app.handle(key(KeyCode::Char('/')), &screen.choices);
        type_text(&mut app, &screen.choices, "q");
        assert!(app.visible.is_empty());
        assert_eq!(app.handle(key(KeyCode::Enter), &screen.choices), None);
        assert_eq!(app.handle(key(KeyCode::Esc), &screen.choices), None);
        assert_eq!(app.visible.len(), 4);
        assert_eq!(
            app.handle(key(KeyCode::Esc), &screen.choices),
            Some(InteractionOutcome::Back)
        );
    }

    #[test]
    fn slash_is_ignored_when_a_list_is_not_filterable() {
        let screen = screen_of(&["one", "two"], 0);
        let mut app = SelectApp::new(&screen);
        assert_eq!(app.handle(key(KeyCode::Char('/')), &screen.choices), None);
        assert!(!app.typing);
    }

    #[test]
    fn filtering_shows_the_query_and_a_no_matches_row() {
        let screen = models();
        let mut app = SelectApp::new(&screen);
        app.handle(key(KeyCode::Char('/')), &screen.choices);
        type_text(&mut app, &screen.choices, "zzz");
        let rows = render(&screen, &app, 50, 6);
        assert!(rows.iter().any(|row| row == "  No matches"));
        assert!(rows.last().unwrap().starts_with("/ zzz"));

        let idle = SelectApp::new(&screen);
        let rows = render(&screen, &idle, 90, 6);
        assert!(rows.last().unwrap().contains("/ filter"));
    }
}
