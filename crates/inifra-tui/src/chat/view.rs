use super::{ChatApp, Ending, INPUT_ROWS, Role, local, markdown, thinking, wrap::wrap_spans};
use crate::presentation::{Tone, terminal_text, tui_style, tui_surface_style};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, app: &ChatApp) {
    if let Some(switcher) = &app.switcher {
        super::switcher::draw(frame, switcher);
        return;
    }

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
    draw_rule(frame, chunks[1]);
    draw_input(frame, app, chunks[2]);
    draw_rule(frame, chunks[3]);
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

/// Builds every conversation line, wrapped by the caller; `width` sizes local command cards.
fn message_lines(app: &ChatApp, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for message in &app.messages {
        match message.role {
            Role::User => lines.extend(user_lines(&message.text, width)),
            Role::Assistant => lines.extend(assistant_lines(message, width)),
            Role::Local => {
                lines.push(Line::from(Span::styled(
                    "◆ Local",
                    tui_style(Tone::Template, true),
                )));
                lines.extend(local::render(&message.text, usize::from(width)));
            }
            Role::Notice => lines.push(Line::styled(
                terminal_text(&message.text),
                tui_style(Tone::Muted, false),
            )),
            Role::Error => lines.extend(error_lines(&message.text)),
        }
        if !(message.role == Role::Assistant && assistant_lines(message, width).is_empty()) {
            lines.push(Line::from(""));
        }
    }
    if let Some(indicator) = pending_indicator(app) {
        lines.push(indicator);
    }
    lines
}

/// The waiting line, shown while the model has not produced text yet or a task is running.
fn pending_indicator(app: &ChatApp) -> Option<Line<'static>> {
    let waiting_for_text = app.streaming
        && app
            .messages
            .last()
            .is_some_and(|message| message.role == Role::Assistant && message.text.is_empty());
    if !waiting_for_text && app.working.is_none() {
        return None;
    }

    let elapsed = app
        .busy_since
        .map(|since| since.elapsed())
        .unwrap_or_default();
    Some(thinking::indicator(elapsed, app.working.as_deref()))
}

/// The user's message on a tinted block, one blank tinted row above and below.
pub(super) fn user_lines(text: &str, width: u16) -> Vec<Line<'static>> {
    let surface = tui_surface_style();
    let width = usize::from(width.max(4));
    let tinted = surface != Style::default();
    let blank = Line::from(Span::styled(" ".repeat(width), surface));

    let mut lines = Vec::new();
    if tinted {
        lines.push(blank.clone());
    }
    for raw in text.lines() {
        let rows = wrap_spans(
            vec![Span::styled("▎ ", tui_style(Tone::Accent, false))],
            vec![Span::raw(terminal_text(raw))],
            2,
            width,
        );
        for mut spans in rows {
            let used = spans.iter().map(Span::width).sum::<usize>();
            spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
            lines.push(Line::from(
                spans
                    .into_iter()
                    .map(|span| {
                        let style = span.style.patch(surface);
                        Span::styled(span.content, style)
                    })
                    .collect::<Vec<_>>(),
            ));
        }
    }
    if tinted {
        lines.push(blank);
    }
    lines
}

/// An assistant reply: the first line leads with `●` and the rest hang indented under it.
fn assistant_lines(message: &super::Message, width: u16) -> Vec<Line<'static>> {
    let bullet = || vec![Span::styled("● ", tui_style(Tone::Accent, true))];
    let tag = match message.ending {
        Ending::Open => None,
        Ending::Interrupted => Some(Span::styled("interrupted", tui_style(Tone::Muted, false))),
        Ending::Failed => Some(Span::styled("request failed", tui_style(Tone::Error, true))),
    };
    if message.text.is_empty() {
        return tag
            .map(|tag| {
                let mut spans = bullet();
                spans.push(tag);
                vec![Line::from(spans)]
            })
            .unwrap_or_default();
    }

    let available = usize::from(width.max(8));
    let mut lines = Vec::new();
    for (index, line) in markdown::render(&message.text).into_iter().enumerate() {
        let prefix = if index == 0 {
            bullet()
        } else {
            vec![Span::raw("  ")]
        };
        let line_style = line.style;
        let body = line
            .spans
            .into_iter()
            .map(|span| Span::styled(span.content, line_style.patch(span.style)))
            .collect();
        lines.extend(
            wrap_spans(prefix, body, 2, available)
                .into_iter()
                .map(Line::from),
        );
    }
    if let Some(tag) = tag {
        lines.push(Line::from(vec![Span::raw("  "), tag]));
    }
    lines
}

/// Screen rows the whole conversation needs at `width`, counting wrapped lines.
pub(super) fn conversation_rows(app: &ChatApp, width: u16) -> usize {
    let width = usize::from(width.max(1));
    message_lines(app, width as u16)
        .iter()
        .map(|line| line.width().max(1).div_ceil(width))
        .sum()
}

/// Rows taken by the rules, composer, and footer around the conversation.
pub(super) const CHROME_ROWS: usize = INPUT_ROWS + 3;

/// A thin dimmed rule that frames the composer.
fn draw_rule(frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(usize::from(area.width)),
            crate::presentation::tui_dimmed_style(),
        )),
        area,
    );
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

    let text = Text::from(message_lines(app, area.width));
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
            tui_style(Tone::Secondary, false),
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
    // A profile named after its model (`test · OpenAI · test`) would read as a repeat.
    let profile = if app.profile == app.model {
        ""
    } else {
        app.profile.as_str()
    };
    for (text, style) in [
        (profile, tui_style(Tone::Accent, true)),
        (app.provider.as_str(), tui_style(Tone::Secondary, false)),
        (app.model.as_str(), Style::default()),
    ] {
        if text.is_empty() {
            continue;
        }
        if !details.is_empty() {
            details.push(separator());
        }
        details.push(Span::styled(terminal_text(text), style));
    }
    if let Some(effort) = &app.effort {
        details.push(separator());
        details.push(Span::styled(
            format!("{} effort", terminal_text(effort)),
            tui_style(Tone::Secondary, false),
        ));
    }

    if let Some(usage) = usage_label(app) {
        if !details.is_empty() {
            details.push(separator());
        }
        details.push(Span::styled(usage, tui_style(Tone::Muted, false)));
    }

    let busy = app.streaming || app.working.is_some();
    let mut hints = Vec::<Span<'_>>::new();
    if busy {
        if let Some(working) = &app.working {
            hints.push(Span::styled(
                terminal_text(working),
                tui_style(Tone::Secondary, false),
            ));
            hints.push(separator());
        }
        hints.push(Span::styled("Esc", tui_style(Tone::Accent, true)));
        hints.push(Span::styled(" cancel", tui_style(Tone::Muted, false)));
    } else if app.confirmation.is_none() {
        hints.push(Span::styled("Ctrl+O", tui_style(Tone::Accent, true)));
        hints.push(Span::styled(" switch", tui_style(Tone::Muted, false)));
    }

    let left = Line::from(details);
    let right = Line::from(hints);
    let gap = 3;
    if left.width() + gap + right.width() <= usize::from(area.width) {
        frame.render_widget(Paragraph::new(left), area);
        frame.render_widget(Paragraph::new(right).right_aligned(), area);
    } else if busy {
        // Keep the cancel hint reachable when the session details do not fit beside it.
        frame.render_widget(Paragraph::new(right), area);
    } else {
        frame.render_widget(Paragraph::new(left), area);
    }
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
