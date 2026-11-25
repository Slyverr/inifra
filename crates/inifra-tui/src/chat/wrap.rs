//! Word wrapping of styled spans with a hanging indent.

use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

/// Wraps `body` after `prefix` to `available` columns.
///
/// Continuation lines start with `indent` spaces, which lets a bullet or label hold its
/// text in a column. Words wider than a line are left whole for the terminal to break.
pub(super) fn wrap_spans(
    prefix: Vec<Span<'static>>,
    body: Vec<Span<'static>>,
    indent: usize,
    available: usize,
) -> Vec<Vec<Span<'static>>> {
    let prefix_width = prefix
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();

    let mut lines = Vec::new();
    let mut current = prefix;
    let mut used = prefix_width;
    let mut started = false;
    for span in body {
        let style = span.style;
        for word in span.content.split_inclusive(' ') {
            let word_width = UnicodeWidthStr::width(word.trim_end());
            if started && used + word_width > available {
                lines.push(finish(std::mem::take(&mut current)));
                current.push(Span::raw(" ".repeat(indent)));
                used = indent;
            }
            current.push(Span::styled(word.to_owned(), style));
            used += UnicodeWidthStr::width(word);
            started = true;
        }
    }
    lines.push(finish(current));
    lines
}

/// Drops the space left after the last word so a full line does not overflow by one column.
fn finish(mut spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    if let Some(last) = spans.last_mut() {
        last.content = last.content.trim_end().to_owned().into();
    }
    spans
}
