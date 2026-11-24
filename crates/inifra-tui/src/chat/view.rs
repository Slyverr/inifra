use super::{ChatApp, Ending, INPUT_ROWS, Role, markdown};
use crate::presentation::{Tone, terminal_text, tui_style};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, app: &ChatApp) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(INPUT_ROWS as u16),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_messages(frame, app, chunks[0]);
    frame.render_widget(Paragraph::new(""), chunks[1]);
    draw_input(frame, app, chunks[2]);
    frame.render_widget(Paragraph::new(""), chunks[3]);
    draw_footer(frame, app, chunks[4]);

    if app.confirmation.is_none() && !app.streaming && app.working.is_none() && chunks[2].height > 0
    {
        let input_width = chunks[2].width.saturating_sub(2);
        let (row, column) = cursor_position(app, input_width);
        let input_line_count = wrapped_input_lines(&app.input, input_width as usize).len();
        let visible_start = input_line_count.saturating_sub(INPUT_ROWS) as u16;
        let visible_lines = input_line_count.min(INPUT_ROWS) as u16;
        let vertical_padding = chunks[2].height.saturating_sub(visible_lines) / 2;
        if row >= visible_start && row - visible_start + vertical_padding < chunks[2].height {
            frame.set_cursor_position((
                chunks[2].x + 2 + column,
                chunks[2].y + row - visible_start + vertical_padding,
            ));
        }
    }
}

fn draw_messages(frame: &mut Frame<'_>, app: &ChatApp, area: ratatui::layout::Rect) {
    if app.messages.is_empty() {
        let welcome = ratatui::layout::Rect::new(area.x, area.y + area.height / 2, area.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Ask about your project or browse templates. ",
                    tui_style(Tone::Secondary, false),
                ),
                Span::styled("/help", tui_style(Tone::Accent, false)),
                Span::styled(" lists commands.", tui_style(Tone::Secondary, false)),
            ])),
            welcome,
        );
        return;
    }

    let mut lines = Vec::new();
    for message in &app.messages {
        match message.role {
            Role::User => {
                let mut text_lines = message.text.lines();
                if let Some(first) = text_lines.next() {
                    lines.push(Line::from(vec![
                        Span::styled("› ", tui_style(Tone::Accent, true)),
                        Span::styled(terminal_text(first), Style::default()),
                    ]));
                }
                lines.extend(
                    text_lines.map(|line| Line::from(format!("  {}", terminal_text(line)))),
                );
            }
            Role::Assistant => {
                let mut label = vec![Span::styled("Inifra ›", tui_style(Tone::Accent, true))];
                match message.ending {
                    Ending::Open => {}
                    Ending::Interrupted => {
                        label.push(Span::styled("  interrupted", tui_style(Tone::Muted, false)));
                    }
                    Ending::Failed => {
                        label.push(Span::styled(
                            "  request failed",
                            tui_style(Tone::Error, true),
                        ));
                    }
                }
                lines.push(Line::from(label));
                if !message.text.is_empty() {
                    lines.extend(markdown::render(&message.text));
                }
            }
            Role::Notice => lines.push(Line::styled(
                terminal_text(&message.text),
                tui_style(Tone::Muted, false),
            )),
            Role::Error => lines.extend(error_lines(&message.text)),
        }
        lines.push(Line::from(""));
    }

    let text = Text::from(lines);
    let width = usize::from(area.width.max(1));
    let total = text
        .lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width))
        .sum::<usize>();
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let max_scroll = total.saturating_sub(area.height as usize) as u16;
    let scroll = max_scroll.saturating_sub(app.scroll_from_bottom);
    frame.render_widget(paragraph.scroll((scroll, 0)), area);
}

fn draw_input(frame: &mut Frame<'_>, app: &ChatApp, area: ratatui::layout::Rect) {
    if let Some(confirmation) = &app.confirmation {
        let choice = if confirmation.approved {
            "  No     [ Yes ]"
        } else {
            "[ No ]    Yes"
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    terminal_text(&confirmation.prompt),
                    tui_style(Tone::Warning, true),
                ),
                Line::from(choice).style(tui_style(Tone::Accent, true)),
                Line::from("←/→ choose · Enter confirm · Esc cancel"),
            ]),
            area,
        );
        return;
    }

    let visual = wrapped_input_lines(&app.input, area.width.saturating_sub(2) as usize);
    let ghost = ghost_completion(app);
    let visible_start = visual.len().saturating_sub(INPUT_ROWS);
    let visible = visual
        .iter()
        .skip(visible_start)
        .take(INPUT_ROWS)
        .collect::<Vec<_>>();
    let vertical_padding = area.height.saturating_sub(visible.len() as u16) / 2;
    let mut lines = vec![Line::from(""); vertical_padding as usize];

    if app.input.is_empty() && ghost.is_none() {
        let mut spans = vec![Span::styled("›  ", tui_style(Tone::Accent, true))];
        spans.push(Span::styled(
            if app.streaming || app.working.is_some() {
                app.working.as_deref().unwrap_or("Waiting for response…")
            } else {
                "Type a message…"
            },
            tui_style(Tone::Muted, false),
        ));
        lines.push(Line::from(spans));
        lines.resize(area.height as usize, Line::from(""));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }

    let input_chars = &app.input;
    let command_end = input_chars
        .iter()
        .position(|character| character.is_whitespace())
        .unwrap_or(input_chars.len());
    let mut consumed = visual
        .iter()
        .take(visible_start)
        .map(|line| line.len())
        .sum::<usize>();
    for (row, chars) in visible.iter().enumerate() {
        let mut spans = Vec::new();
        if row == 0 {
            spans.push(Span::styled("› ", tui_style(Tone::Accent, true)));
        } else {
            spans.push(Span::raw("  "));
        }

        for character in *chars {
            let style = if input_chars.first() == Some(&'/') && consumed < command_end {
                tui_style(Tone::Accent, true)
            } else {
                Style::default()
            };
            spans.push(Span::styled(terminal_text(&character.to_string()), style));
            consumed += 1;
        }

        if row == 0
            && let Some(ghost) = &ghost
        {
            spans.push(Span::styled(
                terminal_text(ghost),
                tui_style(Tone::Muted, false),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.resize(area.height as usize, Line::from(""));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn wrapped_input_lines(input: &[char], width: usize) -> Vec<Vec<char>> {
    let width = width.max(1);
    let mut lines = vec![Vec::new()];
    for character in input {
        if *character == '\n' {
            lines.push(Vec::new());
        } else {
            if lines.last().is_some_and(|line| line.len() >= width) {
                lines.push(Vec::new());
            }
            lines.last_mut().expect("input has a line").push(*character);
        }
    }
    lines
}

pub(super) fn ghost_completion(app: &ChatApp) -> Option<String> {
    let input = app.input_text();
    if input.trim().is_empty() {
        return None;
    }
    app.completions
        .iter()
        .find(|candidate| candidate.starts_with(&input) && candidate.len() > input.len())
        .map(|candidate| candidate[input.len()..].to_owned())
}

fn cursor_position(app: &ChatApp, width: u16) -> (u16, u16) {
    let width = usize::from(width.max(1));
    let mut row = 0usize;
    let mut column = 0usize;
    for character in app.input.iter().take(app.cursor) {
        if *character == '\n' || column >= width {
            row += 1;
            column = 0;
            if *character == '\n' {
                continue;
            }
        }
        column += 1;
    }
    (
        row.min(u16::MAX as usize) as u16,
        column.min(u16::MAX as usize) as u16,
    )
}

fn draw_footer(frame: &mut Frame<'_>, app: &ChatApp, area: ratatui::layout::Rect) {
    let separator = || Span::styled(" · ", tui_style(Tone::Muted, false));
    let mut details = Vec::<Span<'_>>::new();
    for (text, style) in [
        (&app.profile, tui_style(Tone::Accent, true)),
        (&app.provider, tui_style(Tone::Secondary, false)),
        (&app.model, Style::default()),
    ] {
        if text.is_empty() {
            continue;
        }
        if !details.is_empty() {
            details.push(separator());
        }
        details.push(Span::styled(terminal_text(text), style));
    }

    if let Some(usage) = usage_label(app) {
        if !details.is_empty() {
            details.push(separator());
        }
        details.push(Span::styled(usage, tui_style(Tone::Muted, false)));
    }

    if app.streaming || app.working.is_some() {
        if !details.is_empty() {
            details.push(separator());
        }
        if let Some(working) = &app.working {
            details.push(Span::styled(
                terminal_text(working),
                tui_style(Tone::Secondary, false),
            ));
            details.push(separator());
        }
        details.push(Span::styled("Esc", tui_style(Tone::Accent, true)));
        details.push(Span::raw(" cancel"));
    }
    frame.render_widget(Paragraph::new(Line::from(details)), area);
}

/// Compact token usage: `1.2k in · 340 out`, or a total when the split is unknown.
fn usage_label(app: &ChatApp) -> Option<String> {
    match (app.usage.input_tokens, app.usage.output_tokens) {
        (Some(input), Some(output)) => Some(format!(
            "{} in · {} out",
            compact_count(input),
            compact_count(output)
        )),
        _ => app
            .usage
            .total_tokens
            .map(|total| format!("{} tokens", compact_count(total))),
    }
}

pub(super) fn compact_count(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format_scaled(count, 1_000, "k"),
        _ => format_scaled(count, 1_000_000, "M"),
    }
}

fn format_scaled(count: u64, unit: u64, suffix: &str) -> String {
    let tenths = (count * 10 + unit / 2) / unit;
    if tenths.is_multiple_of(10) || tenths >= 1_000 {
        format!("{}{suffix}", tenths / 10)
    } else {
        format!("{}.{}{suffix}", tenths / 10, tenths % 10)
    }
}

/// Renders an error: a red summary, then guidance with `commands` highlighted and a muted
/// `details:` line.
pub(super) fn error_lines(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index == 0 {
            lines.push(Line::styled(
                terminal_text(line),
                tui_style(Tone::Error, true),
            ));
        } else if let Some(details) = line.strip_prefix("details: ") {
            lines.push(Line::from(vec![
                Span::styled("  details: ", tui_style(Tone::Muted, false)),
                Span::styled(terminal_text(details), tui_style(Tone::Muted, false)),
            ]));
        } else {
            let mut spans = vec![Span::raw("  ")];
            for (part_index, part) in line.split('`').enumerate() {
                if part.is_empty() {
                    continue;
                }
                spans.push(Span::styled(
                    terminal_text(part),
                    if part_index % 2 == 1 {
                        tui_style(Tone::Accent, false)
                    } else {
                        tui_style(Tone::Secondary, false)
                    },
                ));
            }
            lines.push(Line::from(spans));
        }
    }
    lines
}
