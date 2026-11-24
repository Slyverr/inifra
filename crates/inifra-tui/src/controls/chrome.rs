//! Shared headings, rows, and key-hint footers for inline controls.

use crate::presentation::{Tone, terminal_text, tui_style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// The heading line of an inline screen: `Inifra · task · context…`.
///
/// The task names what the user is doing; context parts name what the screen applies to, such as
/// a profile ID or provider. An empty task omits the heading row entirely.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Heading {
    task: String,
    context: Vec<String>,
}

impl Heading {
    /// Creates a heading for one task, such as `choose assistant connection`.
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            context: Vec::new(),
        }
    }

    /// Creates an empty heading; the screen then has no heading row.
    pub fn none() -> Self {
        Self::default()
    }

    /// Appends a context part shown after the task in the terminal's default color.
    #[must_use]
    pub fn context(mut self, part: impl Into<String>) -> Self {
        let part = part.into();
        if !part.trim().is_empty() {
            self.context.push(part);
        }
        self
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.task.trim().is_empty()
    }

    pub(crate) fn height(&self) -> u16 {
        u16::from(!self.is_empty())
    }

    /// Renders the heading, with an optional muted note aligned to the right edge.
    pub(crate) fn line(&self, aside: Option<&str>, width: u16) -> Line<'static> {
        if self.is_empty() {
            return Line::default();
        }

        let mut spans = vec![
            Span::styled("Inifra", tui_style(Tone::Accent, true)),
            separator(),
            Span::styled(terminal_text(&self.task), tui_style(Tone::Accent, true)),
        ];
        for part in &self.context {
            spans.push(separator());
            spans.push(Span::raw(terminal_text(part)));
        }

        if let Some(aside) = aside {
            let used = Line::from(spans.clone()).width();
            let aside_width = UnicodeWidthStr::width(aside);
            let available = usize::from(width);
            if used + aside_width + 2 <= available {
                spans.push(Span::raw(" ".repeat(available - used - aside_width)));
                spans.push(Span::styled(
                    aside.to_owned(),
                    tui_style(Tone::Muted, false),
                ));
            }
        }
        Line::from(spans)
    }
}

/// A short colored tag shown after a row, such as `default` or `not set`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Badge {
    /// Tag text.
    pub text: String,
    /// Semantic color of the tag.
    pub tone: Tone,
}

impl Badge {
    /// Creates a tag with a semantic color.
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

/// One aligned row in a choice list or read-only summary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Row {
    /// Primary text; labels in one list are padded to a shared width.
    pub label: String,
    /// Secondary detail shown after the label.
    pub detail: Option<String>,
    /// Optional colored tag shown after the detail.
    pub badge: Option<Badge>,
}

impl Row {
    /// Creates a row with only a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            badge: None,
        }
    }

    /// Adds secondary detail.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        self.detail = (!detail.trim().is_empty()).then_some(detail);
        self
    }

    /// Adds a colored tag.
    #[must_use]
    pub fn badge(mut self, badge: Badge) -> Self {
        self.badge = Some(badge);
        self
    }
}

impl From<String> for Row {
    fn from(label: String) -> Self {
        Self::new(label)
    }
}

impl From<&str> for Row {
    fn from(label: &str) -> Self {
        Self::new(label)
    }
}

/// How one row in a list is emphasized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RowState {
    /// The keyboard cursor is on this row.
    Focused,
    /// A choice that can be focused but is not; shown in the secondary color.
    Idle,
    /// A read-only summary row.
    Summary,
}

/// Renders rows with a two-column cursor gutter and labels padded to `label_width`.
pub(crate) fn row_line(row: &Row, state: RowState, label_width: usize) -> Line<'static> {
    let label = terminal_text(&row.label);
    let padding = label_width.saturating_sub(UnicodeWidthStr::width(label.as_str()));
    let mut spans = vec![match state {
        RowState::Focused => Span::styled("› ", tui_style(Tone::Accent, true)),
        RowState::Idle | RowState::Summary => Span::raw("  "),
    }];
    spans.push(match state {
        RowState::Focused => Span::styled(label, tui_style(Tone::Accent, true)),
        RowState::Idle => Span::styled(label, tui_style(Tone::Secondary, false)),
        RowState::Summary => Span::raw(label),
    });

    if row.detail.is_some() || row.badge.is_some() {
        spans.push(Span::raw(" ".repeat(padding + 3)));
    }
    if let Some(detail) = &row.detail {
        spans.push(Span::styled(
            terminal_text(detail),
            tui_style(Tone::Secondary, false),
        ));
    }
    if let Some(badge) = &row.badge {
        if row.detail.is_some() {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            terminal_text(&badge.text),
            tui_style(badge.tone, false),
        ));
    }
    Line::from(spans)
}

/// Returns the widest label in `rows`, measured in terminal columns.
pub(crate) fn label_width(rows: &[Row]) -> usize {
    rows.iter()
        .map(|row| UnicodeWidthStr::width(terminal_text(&row.label).as_str()))
        .max()
        .unwrap_or_default()
}

/// One key hint in a footer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Hint {
    keys: &'static str,
    label: &'static str,
    essential: bool,
}

/// A hint dropped first when the footer does not fit.
pub(crate) const fn hint(keys: &'static str, label: &'static str) -> Hint {
    Hint {
        keys,
        label,
        essential: false,
    }
}

/// A hint kept for as long as possible when the footer does not fit.
pub(crate) const fn essential(keys: &'static str, label: &'static str) -> Hint {
    Hint {
        keys,
        label,
        essential: true,
    }
}

/// Builds a one-row footer: bold accent keys, default-color labels, and ` · ` separators.
///
/// Optional hints are dropped when the row is too wide, then essential hints lose their labels, so
/// the footer never wraps.
pub(crate) fn footer(hints: &[Hint], width: u16) -> Line<'static> {
    let width = usize::from(width);
    let full = hint_line(hints.iter(), true);
    if full.width() <= width {
        return full;
    }

    let essential = hint_line(hints.iter().filter(|hint| hint.essential), true);
    if essential.width() <= width {
        return essential;
    }

    hint_line(hints.iter().filter(|hint| hint.essential), false)
}

fn hint_line<'a>(hints: impl Iterator<Item = &'a Hint>, labels: bool) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, hint) in hints.enumerate() {
        if index > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::styled(hint.keys, tui_style(Tone::Accent, true)));
        if labels && !hint.label.is_empty() {
            spans.push(Span::raw(format!(" {}", hint.label)));
        }
    }
    Line::from(spans)
}

fn separator() -> Span<'static> {
    Span::styled(" · ", tui_style(Tone::Muted, false))
}

#[cfg(test)]
mod tests {
    use super::{Badge, Heading, Row, RowState, essential, footer, hint, label_width, row_line};
    use crate::presentation::Tone;

    fn text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn heading_joins_task_and_context_and_right_aligns_the_aside() {
        let heading = Heading::new("new profile")
            .context("OpenAI")
            .context("Responses");
        let line = heading.line(Some("step 1 of 3"), 60);
        let rendered = text(&line);
        assert!(rendered.starts_with("Inifra · new profile · OpenAI · Responses"));
        assert!(rendered.ends_with("step 1 of 3"));
        assert_eq!(line.width(), 60);
    }

    #[test]
    fn heading_drops_the_aside_when_it_would_crowd_the_title() {
        let line = Heading::new("new profile").line(Some("step 1 of 3"), 24);
        assert_eq!(text(&line), "Inifra · new profile");
    }

    #[test]
    fn empty_heading_has_no_row() {
        assert_eq!(Heading::none().height(), 0);
        assert_eq!(Heading::new("task").height(), 1);
    }

    #[test]
    fn focused_rows_have_a_cursor_glyph_and_details_align() {
        let rows = [
            Row::new("local").detail("Ollama"),
            Row::new("work-laptop")
                .detail("OpenAI")
                .badge(Badge::new("default", Tone::Success)),
        ];
        let width = label_width(&rows);
        let first = text(&row_line(&rows[0], RowState::Focused, width));
        let second = text(&row_line(&rows[1], RowState::Idle, width));
        assert!(first.starts_with("› local"));
        assert!(second.starts_with("  work-laptop"));
        let column = |text: &str, needle: &str| text[..text.find(needle).unwrap()].chars().count();
        assert_eq!(column(&first, "Ollama"), column(&second, "OpenAI"));
        assert!(second.ends_with("OpenAI   default"));
    }

    #[test]
    fn footer_drops_optional_hints_then_labels_to_stay_on_one_row() {
        let hints = [
            essential("↑/↓", "move"),
            hint("/", "filter"),
            essential("Enter", "choose"),
            essential("Esc", "back"),
        ];
        assert_eq!(
            text(&footer(&hints, 80)),
            "↑/↓ move · / filter · Enter choose · Esc back"
        );
        assert_eq!(
            text(&footer(&hints, 36)),
            "↑/↓ move · Enter choose · Esc back"
        );
        assert_eq!(text(&footer(&hints, 20)), "↑/↓ · Enter · Esc");
    }
}
