//! The animated "waiting for the model" line.

use crate::presentation::{Tone, tui_style};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::time::Duration;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const WORDS: [&str; 5] = [
    "Thinking",
    "Reasoning",
    "Working it out",
    "Gathering context",
    "Putting it together",
];
const FRAME: Duration = Duration::from_millis(80);
const SHIMMER_STEP: Duration = Duration::from_millis(60);
const WORD_TIME: Duration = Duration::from_secs(3);
/// Columns the highlight travels past each end so it enters and leaves the text smoothly.
const SHIMMER_MARGIN: i64 = 4;

/// Builds the waiting line for `elapsed` time since the wait began.
///
/// A spinner leads, followed by `status` when the application names what it is doing, or a
/// word that changes every few seconds. A bright band sweeps across the text, and a
/// seconds counter appears once the wait is no longer brief.
pub(super) fn indicator(elapsed: Duration, status: Option<&str>) -> Line<'static> {
    let frame = (elapsed.as_millis() / FRAME.as_millis()) as usize % SPINNER.len();
    let word = WORDS[(elapsed.as_millis() / WORD_TIME.as_millis()) as usize % WORDS.len()];
    let label = format!("{}…", status.unwrap_or(word));

    let length = label.chars().count() as i64;
    let sweep = (elapsed.as_millis() / SHIMMER_STEP.as_millis()) as i64;
    let center = sweep % (length + 2 * SHIMMER_MARGIN) - SHIMMER_MARGIN;
    let dim = tui_style(Tone::Secondary, false);
    let bright = Style::default().add_modifier(Modifier::BOLD);

    let mut spans = vec![Span::styled(
        format!("{} ", SPINNER[frame]),
        tui_style(Tone::Accent, true),
    )];
    spans.extend(label.chars().enumerate().map(|(index, character)| {
        let near = (index as i64 - center).abs() <= 1;
        Span::styled(character.to_string(), if near { bright } else { dim })
    }));
    if elapsed >= Duration::from_secs(2) {
        spans.push(Span::styled(
            format!("  {}s", elapsed.as_secs()),
            tui_style(Tone::Muted, false),
        ));
    }
    Line::from(spans)
}
