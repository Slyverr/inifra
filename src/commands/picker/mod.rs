use std::io::{self, Write};

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
use crossterm::{queue, style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::templates::Template;

mod layout;
mod preview;
mod state;
mod view;
use state::{Picker, template_rows, templates_in_picker_order};
use view::TerminalSession;

#[derive(Clone, Copy)]
pub(super) struct TextStyle {
    color: Color,
    bold: bool,
    dim: bool,
}

pub(super) mod text {
    pub const APP_NAME: &str = "Inifra";
    pub const CHOOSE_FILES: &str = "choose project files";
    pub const CHOOSE_TEMPLATE: &str = "choose a template";
    pub const SELECTED_SUFFIX: &str = " selected";
    pub const NO_TEMPLATES: &str = "no templates are available";
    pub const NO_MATCHES: &str = "No templates match this filter";
    pub const FILTER_PROMPT: &str = "Filter: ";
    pub const NOTHING_SELECTED: &str = "Select at least one template before continuing.";
    pub const OUTPUT_CONFLICT_NOTICE: &str = " both write ";
    pub const CONFLICT_NOTICE_SUFFIX: &str = "; deselect one before continuing";
    pub const DESCRIPTION: &str = "Description";
    pub const ORIGIN: &str = "Origin";
    pub const FILES: &str = "Files";
    pub const TEMPLATE_VALUES: &str = "Template values";
    pub const SOURCE_PREVIEW: &str = "Source preview";
    pub const EMPTY_FILE: &str = "<empty>";
    pub const BINARY_FILE_PREFIX: &str = "<binary file: ";
    pub const BINARY_FILE_SUFFIX: &str = " bytes>";
    pub const OTHER_CATEGORY: &str = "Other";
    pub const SOURCE_FILE_PREFIX: &str = "--- ";
    pub const SOURCE_FILE_SUFFIX: &str = " ---";
    pub const CURSOR: &str = "›";
    pub const SELECTED_MARK: &str = "●";
    pub const UNSELECTED_MARK: &str = "○";
    pub const TEMPLATE_VARIABLE_OPEN: &str = "{{";
    pub const TEMPLATE_BLOCK_OPEN: &str = "{%";
    pub const TEMPLATE_VARIABLE_CLOSE: &str = "}}";
    pub const TEMPLATE_BLOCK_CLOSE: &str = "%}";
    pub const TITLE_SEPARATOR: &str = " · ";
    pub const TITLE_COUNT_SEPARATOR: &str = "  ·  ";
    pub const INDENT: &str = "  ";
    pub const SPACE: &str = " ";
    pub const MARKDOWN_HEADING_PREFIX: char = '#';
    pub const MARKDOWN_BULLET_PREFIXES: [&str; 2] = ["- ", "* "];
    pub const CATEGORY_TOGGLE_KEY: char = 'a';
    pub const ALTERNATIVE_PREFIX: &str = "‹ ";
    pub const ALTERNATIVE_SUFFIX: &str = " ›";
    pub const ALTERNATIVE_SEPARATOR: &str = "  ·  ";
    pub const FOOTER_VARIANT: (&str, &str) = ("←/→ h/l", " cycle · ");
    pub const FOOTER_SINGLE: [(&str, &str); 7] = [
        ("/", " filter · "),
        ("↑/↓ j/k", " move · "),
        ("Space", " toggle · "),
        ("Tab/⇧Tab", " group · "),
        ("PgUp/Dn", " preview · "),
        ("Enter", " next · "),
        ("Esc", " cancel"),
    ];
    pub const FOOTER_MULTI: [(&str, &str); 8] = [
        ("/", " filter · "),
        ("↑/↓ j/k", " move · "),
        ("Space", " toggle · "),
        ("a", " toggle group · "),
        ("Tab/⇧Tab", " group · "),
        ("PgUp/Dn", " preview · "),
        ("Enter", " next · "),
        ("Esc", " cancel"),
    ];
    pub const FOOTER_SEARCH: [(&str, &str); 5] = [
        ("↑/↓", " move · "),
        ("Text", " filter · "),
        ("Backspace", " delete · "),
        ("Enter", " next · "),
        ("Esc", " finish filter"),
    ];
    pub const FOOTER_CLEAR_FILTER: (&str, &str) = ("Esc", " clear filter");
}

const DEFAULT_SELECTED_TEMPLATE_IDS: &[&str] = &["readme"];

pub(super) mod palette {
    use super::{Color, TextStyle};

    pub const TITLE: TextStyle = TextStyle {
        color: Color::Cyan,
        bold: true,
        dim: false,
    };
    pub const ACTIVE_TEMPLATE: TextStyle = TITLE;
    pub const INACTIVE_TEMPLATE: TextStyle = TextStyle {
        color: Color::Reset,
        bold: false,
        dim: false,
    };
    pub const SELECTED_MARKER: TextStyle = TextStyle {
        color: Color::Green,
        bold: false,
        dim: false,
    };
    pub const UNSELECTED_MARKER: TextStyle = TextStyle {
        color: Color::DarkGrey,
        bold: false,
        dim: false,
    };
    pub const MUTED_HEADING: TextStyle = TextStyle {
        color: Color::Reset,
        bold: false,
        dim: true,
    };
    pub const FILE_PATH: TextStyle = TextStyle {
        color: Color::Green,
        bold: false,
        dim: false,
    };
    pub const TEMPLATE_VALUE: TextStyle = TextStyle {
        color: Color::Yellow,
        bold: false,
        dim: false,
    };
    pub const SOURCE_FILE: TextStyle = TextStyle {
        color: Color::Green,
        bold: true,
        dim: false,
    };
    pub const SOURCE_TEXT: TextStyle = TextStyle {
        color: Color::Reset,
        bold: false,
        dim: false,
    };
    pub const SOURCE_HEADING: TextStyle = TextStyle {
        color: Color::Yellow,
        bold: true,
        dim: false,
    };
    pub const LIST_MARKER: TextStyle = SELECTED_MARKER;
    pub const TEMPLATE_EXPRESSION: TextStyle = TextStyle {
        color: Color::Magenta,
        bold: true,
        dim: false,
    };
    pub const NOTICE: TextStyle = TEMPLATE_VALUE;
    pub const FOOTER_KEY: TextStyle = TITLE;
    pub const FOOTER_TEXT: TextStyle = SOURCE_TEXT;
}

pub(super) fn choose(catalog: &[Template]) -> Result<Option<Vec<String>>> {
    let mut ordered = catalog.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        let left_is_default = DEFAULT_SELECTED_TEMPLATE_IDS.contains(&left.id.as_str());
        let right_is_default = DEFAULT_SELECTED_TEMPLATE_IDS.contains(&right.id.as_str());
        right_is_default
            .cmp(&left_is_default)
            .then_with(|| {
                left.category
                    .as_deref()
                    .unwrap_or(text::OTHER_CATEGORY)
                    .cmp(right.category.as_deref().unwrap_or(text::OTHER_CATEGORY))
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    let templates = templates_in_picker_order(&ordered);
    if templates.is_empty() {
        bail!("{}", text::NO_TEMPLATES);
    }
    let rows = template_rows(&templates);

    let selected = templates
        .iter()
        .map(|template| DEFAULT_SELECTED_TEMPLATE_IDS.contains(&template.id.as_str()))
        .collect::<Vec<_>>();
    let choices: Vec<usize> = rows
        .iter()
        .map(|row| {
            row.alternatives
                .iter()
                .copied()
                .find(|index| selected[*index])
                .unwrap_or(row.alternatives[0])
        })
        .collect();
    let cursor = rows
        .iter()
        .position(|row| row.alternatives.iter().any(|index| selected[*index]))
        .unwrap_or_default();
    let mut picker = Picker {
        selected,
        choices,
        cursor,
        preview_scroll: 0,
        multiple: true,
        notice: None,
        query: String::new(),
        search_mode: false,
    };
    let session = TerminalSession::enter()?;

    loop {
        let visible_rows = picker.visible_rows(&templates, &rows);
        picker.draw(&templates, &rows, &visible_rows, &session)?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('/') if !picker.search_mode => picker.search_mode = true,
            KeyCode::Char(character) if picker.search_mode => {
                picker.query.push(character);
                picker.update_search(&templates, &rows);
            }
            KeyCode::Backspace if picker.search_mode => {
                picker.query.pop();
                picker.update_search(&templates, &rows);
            }
            KeyCode::Up | KeyCode::Char('k') if !visible_rows.is_empty() => {
                picker.move_up(&visible_rows)
            }
            KeyCode::Down | KeyCode::Char('j') if !visible_rows.is_empty() => {
                picker.move_down(&visible_rows)
            }
            KeyCode::Left | KeyCode::Char('h') if !visible_rows.is_empty() => {
                picker.move_variant(&templates, &rows, true)
            }
            KeyCode::Right | KeyCode::Char('l') if !visible_rows.is_empty() => {
                picker.move_variant(&templates, &rows, false)
            }
            KeyCode::Tab => picker.move_category(&rows, &visible_rows, false),
            KeyCode::BackTab => picker.move_category(&rows, &visible_rows, true),
            KeyCode::Char(key) if key.to_ascii_lowercase() == text::CATEGORY_TOGGLE_KEY => {
                picker.toggle_category(&templates, &rows, &visible_rows)
            }
            KeyCode::PageUp => picker.scroll_preview_up(layout::PREVIEW_SCROLL_ROWS),
            KeyCode::PageDown => picker.scroll_preview_down(layout::PREVIEW_SCROLL_ROWS),
            KeyCode::Char(' ') if !picker.search_mode && !visible_rows.is_empty() => {
                picker.toggle_current(&templates, &rows)
            }
            KeyCode::Enter => {
                picker.search_mode = false;
                if let Some(selected) = picker.confirm(&templates) {
                    return Ok(Some(selected));
                }
            }
            KeyCode::Esc if picker.search_mode => picker.search_mode = false,
            KeyCode::Esc if !picker.query.is_empty() => picker.clear_search(&templates, &rows),
            KeyCode::Esc => return Ok(None),
            _ => {}
        }
    }
}

fn terminal_row(row: usize) -> u16 {
    row.min(usize::from(u16::MAX)) as u16
}

pub(super) fn colors_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && console::colors_enabled()
}

pub(super) fn set_style(
    stdout: &mut impl Write,
    enabled: bool,
    text_style: TextStyle,
) -> io::Result<()> {
    if enabled {
        queue!(
            stdout,
            SetAttribute(Attribute::Reset),
            SetForegroundColor(text_style.color)
        )?;
        if text_style.bold {
            queue!(stdout, SetAttribute(Attribute::Bold))?;
        }
        if text_style.dim {
            queue!(stdout, SetAttribute(Attribute::Dim))?;
        }
    }
    Ok(())
}

pub(super) fn reset_style(stdout: &mut impl Write, enabled: bool) -> io::Result<()> {
    if enabled {
        queue!(stdout, style::ResetColor, SetAttribute(Attribute::Reset))?;
    }
    Ok(())
}

pub(super) fn truncate(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut width = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + character_width > max_width.saturating_sub(1) {
            break;
        }
        result.push(character);
        width += character_width;
    }
    result.push('…');
    result
}
