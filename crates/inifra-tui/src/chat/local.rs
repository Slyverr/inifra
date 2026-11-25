//! Styling for the output of local slash commands.
//!
//! Commands return plain text, which is also what the model sees as context, so this module
//! recovers structure from the shape of each line instead of changing the text.

use super::{markdown, wrap::wrap_spans};
use crate::presentation::{Tone, terminal_text, tui_style};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

const GUTTER: &str = "▏ ";
const MIN_BODY_WIDTH: usize = 8;
/// A command's description moves under it when fewer than this many columns remain beside it.
const MIN_DESCRIPTION_WIDTH: usize = 24;
/// Plan rows that begin with one of these actions, followed by ` · `.
const ACTIONS: &[(&str, Tone)] = &[
    ("create directory", Tone::Success),
    ("create", Tone::Success),
    ("replace", Tone::Warning),
    ("already exists", Tone::Muted),
];

/// Renders command output as a card: a left gutter, colored by line shape, wrapped to `width`.
pub(super) fn render(text: &str, width: usize) -> Vec<Line<'static>> {
    if text.trim_start().starts_with('#') {
        return markdown::render(text)
            .into_iter()
            .map(|line| with_gutter(line.spans))
            .collect();
    }

    let width = width.max(UnicodeWidthStr::width(GUTTER) + MIN_BODY_WIDTH);
    let mut lines = Vec::new();
    let mut previous_blank = true;
    for (index, raw) in text.lines().enumerate() {
        let raw = terminal_text(raw);
        if raw.trim().is_empty() {
            lines.push(with_gutter(Vec::new()));
            previous_blank = true;
            continue;
        }
        let row = classify(index, &raw, previous_blank);
        lines.extend(wrap(row, width));
        previous_blank = false;
    }
    lines
}

/// One logical row: an unwrapped prefix, a wrappable body, and extra continuation indent.
struct Row {
    prefix: Vec<Span<'static>>,
    body: Vec<Span<'static>>,
    hang: usize,
    /// Whether the body may drop to its own line under the first prefix span on narrow screens.
    stackable: bool,
}

fn row(prefix: Vec<Span<'static>>, body: Vec<Span<'static>>, hang: usize) -> Row {
    Row {
        prefix,
        body,
        hang,
        stackable: false,
    }
}

fn span(text: impl Into<String>, style: Style) -> Span<'static> {
    Span::styled(text.into(), style)
}

fn classify(index: usize, raw: &str, previous_blank: bool) -> Row {
    let plain = Style::default();
    let secondary = tui_style(Tone::Secondary, false);
    let muted = tui_style(Tone::Muted, false);

    if index == 0 && !raw.starts_with('/') {
        return row(
            Vec::new(),
            vec![span(raw, tui_style(Tone::Accent, true))],
            0,
        );
    }

    // `/command <arg>   description`: keep the description column and wrap under it.
    if raw.starts_with('/')
        && let Some(gap) = raw.find("  ")
    {
        let rest = raw[gap..].trim_start();
        let column = raw.len() - rest.len();
        return Row {
            stackable: true,
            ..row(
                vec![
                    span(&raw[..gap], tui_style(Tone::Accent, false)),
                    span(" ".repeat(column - gap), plain),
                ],
                vec![span(rest, secondary)],
                0,
            )
        };
    }

    // A one-word capitalized line after a blank line heads a section such as `Files`.
    if previous_blank
        && index > 0
        && raw.chars().next().is_some_and(char::is_uppercase)
        && !raw.contains(char::is_whitespace)
    {
        return row(
            Vec::new(),
            vec![span(raw, tui_style(Tone::Secondary, true))],
            0,
        );
    }

    if raw.starts_with('…') {
        return row(Vec::new(), vec![span(raw, muted)], 0);
    }

    if raw.starts_with("Applied ") {
        return row(
            Vec::new(),
            vec![span(raw, tui_style(Tone::Success, false))],
            0,
        );
    }

    if let Some((action, rest)) = raw.split_once(" · ")
        && let Some((_, tone)) = ACTIONS.iter().find(|(name, _)| *name == action)
    {
        return row(
            vec![span(action, tui_style(*tone, false)), span(" · ", muted)],
            vec![span(rest, plain)],
            0,
        );
    }

    if raw.contains(" · ") {
        let mut parts = raw.split(" · ");
        let mut body = vec![span(
            parts.next().unwrap_or_default(),
            tui_style(Tone::Accent, false),
        )];
        for part in parts {
            body.push(span(" · ", muted));
            body.push(span(part, secondary));
        }
        return row(Vec::new(), body, 2);
    }

    if let Some((key, value)) = raw.split_once(": ")
        && UnicodeWidthStr::width(key) <= 24
    {
        return row(
            vec![span(format!("{key}: "), secondary)],
            vec![span(value, plain)],
            0,
        );
    }

    row(Vec::new(), vec![span(raw, plain)], 0)
}

fn with_gutter(mut spans: Vec<Span<'static>>) -> Line<'static> {
    spans.insert(0, span(GUTTER, tui_style(Tone::Muted, false)));
    Line::from(spans)
}

/// Wraps `row.body` after its prefix, indenting continuation lines under it.
fn wrap(row: Row, width: usize) -> Vec<Line<'static>> {
    let available = width.saturating_sub(UnicodeWidthStr::width(GUTTER));
    let prefix_width = row
        .prefix
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    if row.stackable && available.saturating_sub(prefix_width) < MIN_DESCRIPTION_WIDTH {
        let mut prefix = row.prefix;
        prefix.truncate(1);
        let mut lines = vec![with_gutter(prefix)];
        lines.extend(
            wrap_spans(vec![Span::raw("  ")], row.body, 2, available)
                .into_iter()
                .map(with_gutter),
        );
        return lines;
    }

    // Leave room for the body even when a long prefix would otherwise squeeze it out.
    let indent = (prefix_width + row.hang).min(available.saturating_sub(MIN_BODY_WIDTH));

    wrap_spans(row.prefix, row.body, indent, available)
        .into_iter()
        .map(with_gutter)
        .collect()
}
