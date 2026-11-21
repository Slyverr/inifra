use crate::{
    TemplateView,
    presentation::{Tone, terminal_text, tui_style},
};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use std::{collections::BTreeMap, path::PathBuf};

const VALUE_INDENT: &str = "  ";

#[derive(Clone)]
struct PreviewLine {
    line: Line<'static>,
    section: Option<String>,
}

pub(super) fn draw(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    template: &TemplateView,
    resolved: Option<&BTreeMap<PathBuf, PathBuf>>,
    scroll: u16,
) {
    if area.is_empty() {
        return;
    }

    let lines = preview_lines(template, resolved);
    let width = area.width.max(1);
    let visual_starts = visual_section_starts(&lines, width);
    let visual_height = lines
        .iter()
        .map(|line| wrapped_line_count(&line.line, width))
        .sum::<usize>();
    let body_height = usize::from(area.height.saturating_sub(1)).max(1);
    let max_scroll = visual_height.saturating_sub(body_height);
    let scroll = usize::from(scroll).min(max_scroll);

    let active_section = active_section(&visual_starts, scroll);
    if let Some(title) = active_section {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title.to_owned(),
                crate::presentation::tui_dimmed_style(),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    let body_scroll = if visual_starts.iter().any(|(start, _)| *start == scroll) {
        scroll.saturating_add(1)
    } else {
        scroll
    };

    let body = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    );
    let text = Text::from(lines.into_iter().map(|line| line.line).collect::<Vec<_>>());
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((body_scroll.min(u16::MAX as usize) as u16, 0)),
        body,
    );
}

fn preview_lines(
    template: &TemplateView,
    resolved: Option<&BTreeMap<PathBuf, PathBuf>>,
) -> Vec<PreviewLine> {
    let mut lines = Vec::new();
    push_section(
        &mut lines,
        "Description",
        [regular_line(terminal_text(&template.description))],
    );
    if !template.sources.is_empty() {
        push_section(
            &mut lines,
            "Files",
            template.sources.iter().map(|source| {
                colored_line(
                    format!(
                        "{VALUE_INDENT}{}",
                        terminal_text(&source.resolved_output(resolved).display().to_string())
                    ),
                    Tone::Success,
                    false,
                )
            }),
        );
    }

    if !template.directories.is_empty() {
        push_section(
            &mut lines,
            "Directories",
            template.directories.iter().map(|path| {
                colored_line(
                    format!(
                        "{VALUE_INDENT}{}/",
                        terminal_text(&path.display().to_string())
                    ),
                    Tone::Success,
                    false,
                )
            }),
        );
    }

    if !template.inputs.is_empty() {
        push_section(
            &mut lines,
            "Template inputs",
            template.inputs.iter().map(|input| PreviewLine {
                line: Line::from(vec![
                    Span::styled(
                        format!("{VALUE_INDENT}{}", terminal_text(&input.name)),
                        tui_style(Tone::Accent, true),
                    ),
                    Span::styled(
                        format!(" · {}", terminal_text(&input.prompt)),
                        tui_style(Tone::Secondary, false),
                    ),
                ]),
                section: None,
            }),
        );
    }

    let mut source_lines = Vec::new();
    for (index, source) in template.sources.iter().enumerate() {
        if index > 0 {
            source_lines.push(regular_line(String::new()));
        }
        source_lines.push(colored_line(
            terminal_text(&format!(
                "--- {} ---",
                source.resolved_output(resolved).display()
            )),
            Tone::Success,
            true,
        ));
        if source.content.is_empty() {
            source_lines.push(regular_line("<empty>".to_owned()));
        } else {
            source_lines.extend(
                source
                    .content
                    .lines()
                    .map(|line| source_line(&terminal_text(line))),
            );
        }
    }

    if !source_lines.is_empty() {
        push_section(&mut lines, "Source preview", source_lines);
    }
    lines
}

fn push_section(
    lines: &mut Vec<PreviewLine>,
    title: &str,
    content: impl IntoIterator<Item = PreviewLine>,
) {
    if !lines.is_empty() {
        lines.push(regular_line(String::new()));
    }
    lines.push(PreviewLine {
        line: Line::from(Span::styled(
            title.to_owned(),
            crate::presentation::tui_dimmed_style(),
        )),
        section: Some(title.to_owned()),
    });
    lines.extend(content);
}

fn regular_line(text: String) -> PreviewLine {
    PreviewLine {
        line: Line::from(Span::styled(text, Style::default())),
        section: None,
    }
}

fn colored_line(text: String, tone: Tone, bold: bool) -> PreviewLine {
    PreviewLine {
        line: Line::from(Span::styled(text, tui_style(tone, bold))),
        section: None,
    }
}

fn source_line(text: &str) -> PreviewLine {
    let base = if text.trim_start().starts_with('#') {
        tui_style(Tone::Warning, true)
    } else {
        Style::default()
    };

    let marker = if text.starts_with("- ") || text.starts_with("* ") {
        Some(2)
    } else {
        None
    };

    let mut spans = Vec::new();
    if let Some(marker_end) = marker {
        spans.push(Span::styled(
            text[..marker_end].to_owned(),
            tui_style(Tone::Success, false),
        ));
    }

    let rest = marker.map_or(text, |end| &text[end..]);
    append_template_spans(rest, base, &mut spans);
    PreviewLine {
        line: Line::from(spans),
        section: None,
    }
}

fn append_template_spans(text: &str, base: Style, spans: &mut Vec<Span<'static>>) {
    let mut rest = text;
    while !rest.is_empty() {
        let variable = rest.find("{{");
        let block = rest.find("{%");
        let next = match (variable, block) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(index), None) | (None, Some(index)) => Some(index),
            (None, None) => None,
        };

        let Some(start) = next else {
            spans.push(Span::styled(rest.to_owned(), base));
            break;
        };

        if start > 0 {
            spans.push(Span::styled(rest[..start].to_owned(), base));
        }

        let opener = if rest[start..].starts_with("{{") {
            ("{{", "}}")
        } else {
            ("{%", "%}")
        };

        let end = rest[start + opener.0.len()..]
            .find(opener.1)
            .map(|offset| start + opener.0.len() + offset + opener.1.len())
            .unwrap_or(rest.len());
        spans.push(Span::styled(
            rest[start..end].to_owned(),
            tui_style(Tone::Accent, true),
        ));
        rest = &rest[end..];
    }
}

fn visual_section_starts(lines: &[PreviewLine], width: u16) -> Vec<(usize, String)> {
    let mut starts = Vec::new();
    let mut visual_row = 0;
    for line in lines {
        if let Some(section) = &line.section {
            starts.push((visual_row, section.clone()));
        }
        visual_row += wrapped_line_count(&line.line, width);
    }
    starts
}

fn active_section(starts: &[(usize, String)], scroll: usize) -> Option<&str> {
    starts
        .iter()
        .rev()
        .find(|(start, _)| *start <= scroll)
        .map(|(_, title)| title.as_str())
}

fn wrapped_line_count(line: &Line<'static>, width: u16) -> usize {
    line.width().max(1).div_ceil(usize::from(width.max(1)))
}

#[cfg(test)]
mod tests {
    use super::{active_section, preview_lines, visual_section_starts};
    use crate::{
        TemplateInputView, TemplateSourceView, TemplateView,
        presentation::{Tone, tui_dimmed_style, tui_style},
    };
    use ratatui::{style::Style, text::Line};

    fn template() -> TemplateView {
        TemplateView {
            id: "guide".into(),
            description: "Root guidance".into(),
            inputs: vec![TemplateInputView {
                name: "project_name".into(),
                prompt: "Project name".into(),
            }],
            directories: vec![],
            sources: vec![TemplateSourceView {
                source: "README.md.j2".into(),
                output: "README.md".into(),
                alternate_output: None,
                content: "# Guide\n\nOrdinary text\n- Name: {{ project_name }}\n{% if enabled %}"
                    .into(),
            }],
        }
    }

    fn joined(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn preview_restores_sections_and_all_source_highlights() {
        let lines = preview_lines(&template(), None);
        let heading = lines
            .iter()
            .find(|line| joined(&line.line) == "Source preview")
            .unwrap();
        assert_eq!(heading.line.spans[0].style, tui_dimmed_style());
        assert!(
            lines
                .iter()
                .any(|line| joined(&line.line) == "Template inputs")
        );
        assert!(
            lines
                .iter()
                .any(|line| joined(&line.line) == "--- README.md ---")
        );
        assert!(lines.iter().any(|line| joined(&line.line) == "  README.md"));
        let input_line = lines
            .iter()
            .find(|line| joined(&line.line) == "  project_name · Project name")
            .unwrap();
        assert_eq!(
            input_line.line.spans[0].style,
            tui_style(Tone::Accent, true)
        );
        assert_eq!(
            input_line.line.spans[1].style,
            tui_style(Tone::Secondary, false)
        );

        let markdown_heading = lines
            .iter()
            .find(|line| joined(&line.line) == "# Guide")
            .unwrap();
        assert_eq!(
            markdown_heading.line.spans[0].style,
            tui_style(Tone::Warning, true)
        );

        let list_item = lines
            .iter()
            .find(|line| joined(&line.line).starts_with("- Name:"))
            .unwrap();
        assert_eq!(
            list_item.line.spans[0].style,
            tui_style(Tone::Success, false)
        );
        assert!(list_item.line.spans.iter().any(|span| {
            span.content.as_ref() == "{{ project_name }}"
                && span.style == tui_style(Tone::Accent, true)
        }));
    }

    #[test]
    fn ordinary_source_text_uses_the_terminal_default_style() {
        let lines = preview_lines(&template(), None);
        let ordinary = lines
            .iter()
            .find(|line| joined(&line.line) == "Ordinary text")
            .unwrap();
        assert_eq!(ordinary.line.spans[0].style, Style::default());
    }

    #[test]
    fn section_starts_remain_available_for_sticky_scrolling() {
        let lines = preview_lines(&template(), None);
        let starts = visual_section_starts(&lines, 80);
        let source_start = starts
            .iter()
            .find(|(_, title)| title == "Source preview")
            .unwrap()
            .0;

        assert_eq!(starts[0].1, "Description");
        assert_eq!(
            active_section(&starts, source_start),
            Some("Source preview")
        );
    }
}
