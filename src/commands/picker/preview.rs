use super::{TextStyle, palette, reset_style, set_style, text, truncate};
use crate::templates::Template;
use std::io::{self, Write};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy)]
pub(super) enum PreviewStyle {
    Section,
    Text,
    File,
    Variable,
    SourceFile,
    SourceText,
    MarkdownHeading,
    TemplateHeading,
    MarkdownList,
    TemplateValue,
}

pub(super) struct PreviewLine {
    pub(super) text: String,
    pub(super) style: PreviewStyle,
}

pub(super) fn preview_lines(template: &Template) -> Vec<PreviewLine> {
    let mut lines = vec![
        preview_line(text::DESCRIPTION, PreviewStyle::Section),
        preview_line(
            template.description.escape_debug().to_string(),
            PreviewStyle::Text,
        ),
        preview_line(String::new(), PreviewStyle::Text),
        preview_line(text::ORIGIN, PreviewStyle::Section),
        preview_line(template.origin.label(), PreviewStyle::Text),
        preview_line(String::new(), PreviewStyle::Text),
        preview_line(text::FILES, PreviewStyle::Section),
    ];
    for file in &template.files {
        lines.push(preview_line(
            format!(
                "{}{}",
                text::INDENT,
                file.path.display().to_string().escape_debug()
            ),
            PreviewStyle::File,
        ));
    }
    if !template.inputs.is_empty() {
        lines.push(preview_line(String::new(), PreviewStyle::Text));
        lines.push(preview_line(text::TEMPLATE_VALUES, PreviewStyle::Section));
        for input in &template.inputs {
            lines.push(preview_line(
                format!("{}{input}", text::INDENT),
                PreviewStyle::Variable,
            ));
        }
    }
    lines.push(preview_line(String::new(), PreviewStyle::Text));
    lines.push(preview_line(text::SOURCE_PREVIEW, PreviewStyle::Section));
    for file in &template.files {
        lines.push(preview_line(String::new(), PreviewStyle::Text));
        lines.push(preview_line(
            format!(
                "{}{}{}",
                text::SOURCE_FILE_PREFIX,
                file.path.display().to_string().escape_debug(),
                text::SOURCE_FILE_SUFFIX
            ),
            PreviewStyle::SourceFile,
        ));
        match std::str::from_utf8(&file.contents) {
            Ok("") => lines.push(preview_line(text::EMPTY_FILE, PreviewStyle::SourceText)),
            Ok(contents) => lines.extend(contents.lines().map(|line| {
                let text = line.escape_debug().to_string();
                let style = if text.starts_with(text::MARKDOWN_HEADING_PREFIX)
                    && has_template_expressions(&text)
                {
                    PreviewStyle::TemplateHeading
                } else if text.starts_with(text::MARKDOWN_HEADING_PREFIX) {
                    PreviewStyle::MarkdownHeading
                } else if is_markdown_list_item(&text) {
                    PreviewStyle::MarkdownList
                } else if has_template_expressions(&text) {
                    PreviewStyle::TemplateValue
                } else {
                    PreviewStyle::SourceText
                };
                preview_line(text, style)
            })),
            Err(_) => lines.push(preview_line(
                format!(
                    "{}{}{}",
                    text::BINARY_FILE_PREFIX,
                    file.contents.len(),
                    text::BINARY_FILE_SUFFIX
                ),
                PreviewStyle::SourceText,
            )),
        }
    }
    lines
}

fn preview_line(text: impl Into<String>, style: PreviewStyle) -> PreviewLine {
    PreviewLine {
        text: text.into(),
        style,
    }
}

pub(super) fn write_preview_line(
    stdout: &mut impl Write,
    colors: bool,
    line: &PreviewLine,
    max_width: usize,
) -> io::Result<()> {
    let text = truncate(&line.text, max_width);
    let base_style = match line.style {
        PreviewStyle::Section => palette::MUTED_HEADING,
        PreviewStyle::Text | PreviewStyle::SourceText => palette::SOURCE_TEXT,
        PreviewStyle::File => palette::FILE_PATH,
        PreviewStyle::Variable => palette::TEMPLATE_VALUE,
        PreviewStyle::SourceFile => palette::SOURCE_FILE,
        PreviewStyle::MarkdownHeading | PreviewStyle::TemplateHeading => palette::SOURCE_HEADING,
        PreviewStyle::MarkdownList | PreviewStyle::TemplateValue => palette::SOURCE_TEXT,
    };
    set_style(stdout, colors, base_style)?;
    match line.style {
        PreviewStyle::MarkdownList => {
            if let Some((marker, rest)) = markdown_list_item(&text) {
                set_style(stdout, colors, palette::LIST_MARKER)?;
                write!(stdout, "{marker}")?;
                set_style(stdout, colors, base_style)?;
                write!(stdout, "{rest}")?;
            } else {
                write!(stdout, "{text}")?;
            }
        }
        PreviewStyle::TemplateHeading => {
            write_template_highlights(stdout, colors, &text, base_style)?
        }
        PreviewStyle::TemplateValue => {
            write_template_highlights(stdout, colors, &text, base_style)?
        }
        _ => write!(stdout, "{text}")?,
    }
    reset_style(stdout, colors)
}

fn write_template_highlights(
    stdout: &mut impl Write,
    colors: bool,
    text: &str,
    base_style: TextStyle,
) -> io::Result<()> {
    let mut remaining = text;
    while let Some(open) = remaining
        .find(text::TEMPLATE_VARIABLE_OPEN)
        .or_else(|| remaining.find(text::TEMPLATE_BLOCK_OPEN))
    {
        write!(stdout, "{}", &remaining[..open])?;
        let closing = if remaining[open..].starts_with(text::TEMPLATE_VARIABLE_OPEN) {
            text::TEMPLATE_VARIABLE_CLOSE
        } else {
            text::TEMPLATE_BLOCK_CLOSE
        };
        let tail = &remaining[open..];
        let end = tail
            .find(closing)
            .map_or(tail.len(), |index| index + closing.len());
        set_style(stdout, colors, palette::TEMPLATE_EXPRESSION)?;
        write!(stdout, "{}", &tail[..end])?;
        set_style(stdout, colors, base_style)?;
        remaining = &tail[end..];
    }
    write!(stdout, "{remaining}")
}

fn has_template_expressions(text: &str) -> bool {
    text.contains(text::TEMPLATE_VARIABLE_OPEN) || text.contains(text::TEMPLATE_BLOCK_OPEN)
}

fn is_markdown_list_item(line: &str) -> bool {
    markdown_list_item(line).is_some()
}

fn markdown_list_item(line: &str) -> Option<(&'static str, &str)> {
    text::MARKDOWN_BULLET_PREFIXES
        .iter()
        .find_map(|marker| line.strip_prefix(marker).map(|rest| (*marker, rest)))
}

pub(super) fn wrap_line(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut width = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + character_width > max_width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            width = 0;
        }
        line.push(character);
        width += character_width;
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}
