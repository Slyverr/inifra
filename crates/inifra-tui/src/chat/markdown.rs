use crate::presentation::{Tone, terminal_text, tui_style};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub(super) fn render(markdown: &str) -> Vec<Line<'static>> {
    if markdown.is_empty() {
        return vec![Line::from("")];
    }

    let mut in_code = false;
    markdown
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
                return Line::from("");
            }

            if in_code {
                return Line::styled(terminal_text(line), tui_style(Tone::Muted, false));
            }

            let trimmed = line.trim_start();
            let (text, style) = if let Some(heading) = trimmed
                .strip_prefix("### ")
                .or_else(|| trimmed.strip_prefix("## "))
                .or_else(|| trimmed.strip_prefix("# "))
            {
                (heading, tui_style(Tone::Accent, true))
            } else if let Some(item) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                return Line::from(
                    [Span::styled("• ", tui_style(Tone::Success, false))]
                        .into_iter()
                        .chain(inline_spans(item, Style::default()))
                        .collect::<Vec<_>>(),
                );
            } else if let Some(item) = trimmed.strip_prefix("> ") {
                (
                    item,
                    tui_style(Tone::Muted, false).add_modifier(Modifier::ITALIC),
                )
            } else {
                (line, Style::default())
            };
            Line::from(inline_spans(text, style))
        })
        .collect()
}

fn inline_spans(markdown: &str, base_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut style = base_style;
    let mut styles = Vec::new();
    let mut links = Vec::new();

    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(Tag::Strong) => {
                styles.push(style);
                style = style.add_modifier(Modifier::BOLD);
            }
            Event::Start(Tag::Emphasis) => {
                styles.push(style);
                style = style.add_modifier(Modifier::ITALIC);
            }
            Event::Start(Tag::Strikethrough) => {
                styles.push(style);
                style = style.add_modifier(Modifier::CROSSED_OUT);
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                styles.push(style);
                links.push(dest_url.to_string());
                style = tui_style(Tone::Accent, false).add_modifier(Modifier::UNDERLINED);
            }
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                if let Some(previous) = styles.pop() {
                    style = previous;
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some(previous) = styles.pop() {
                    style = previous;
                }

                if let Some(destination) = links.pop() {
                    spans.push(Span::styled(
                        format!(" ({})", terminal_text(&destination)),
                        tui_style(Tone::Muted, false),
                    ));
                }
            }
            Event::Text(text) | Event::Html(text) => {
                spans.push(Span::styled(terminal_text(&text), style));
            }
            Event::Code(code) => spans.push(Span::styled(
                terminal_text(&code),
                tui_style(Tone::Accent, false),
            )),
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                spans.push(Span::styled(terminal_text(&text), style));
            }
            Event::SoftBreak | Event::HardBreak => spans.push(Span::styled(" ", style)),
            Event::Rule => spans.push(Span::styled("─", tui_style(Tone::Muted, false))),
            Event::TaskListMarker(checked) => spans.push(Span::styled(
                if checked { "[x] " } else { "[ ] " },
                tui_style(Tone::Success, false),
            )),
            Event::FootnoteReference(label) => {
                spans.push(Span::styled(format!("[{label}]"), style));
            }
            Event::Start(Tag::Image { .. }) => {
                spans.push(Span::styled("[image: ", tui_style(Tone::Muted, false)))
            }
            Event::End(TagEnd::Image) => {
                spans.push(Span::styled("]", tui_style(Tone::Muted, false)));
            }
            Event::InlineHtml(text) => spans.push(Span::styled(
                terminal_text(&text),
                Style::default().fg(Color::Reset),
            )),
            Event::Start(_) | Event::End(_) => {}
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(terminal_text(markdown)));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::{inline_spans, render};
    use ratatui::style::Modifier;

    fn text(spans: &[ratatui::text::Span<'_>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn renders_inline_emphasis_and_code_without_markers() {
        let spans = inline_spans("Use **bold**, *italic*, and `code`.", Default::default());

        assert_eq!(text(&spans), "Use bold, italic, and code.");
        assert!(spans.iter().any(|span| {
            span.content.as_ref() == "bold" && span.style.add_modifier.contains(Modifier::BOLD)
        }));
        assert!(spans.iter().any(|span| span.content.as_ref() == "code"));
    }

    #[test]
    fn renders_links_and_block_markers_as_readable_terminal_text() {
        let lines = render("## Heading\n- **Item**\n[Docs](https://example.test)");
        let rendered = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(rendered.contains("Heading"));
        assert!(rendered.contains("• Item"));
        assert!(rendered.contains("Docs (https://example.test)"));
        assert!(!rendered.contains("**"));
    }
}
