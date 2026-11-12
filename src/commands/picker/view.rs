use super::preview::{PreviewLine, PreviewStyle, preview_lines, wrap_line, write_preview_line};
use super::state::{Picker, PickerEntry, TemplateRow};
use super::{
    colors_enabled, layout, palette, reset_style, set_style, terminal_row, text, truncate,
};
use crate::templates::Template;
use anyhow::Result;
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{cursor, execute, queue, style};
use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;
impl Picker {
    pub(super) fn draw(
        &mut self,
        templates: &[&Template],
        rows: &[TemplateRow],
        visible_rows: &[usize],
        session: &TerminalSession,
    ) -> Result<()> {
        let entries = picker_entries(rows, visible_rows);
        let cursor_row = entries
            .iter()
            .position(
                |entry| matches!(entry, PickerEntry::Template(index, _) if *index == self.cursor),
            )
            .unwrap_or_default();
        let (width, terminal_height) = terminal::size()?;
        let width = usize::from(width);
        let layout = layout::PickerLayout::calculate(
            width,
            usize::from(terminal_height),
            session.top,
            session.rows,
            entries.len(),
        );
        let start = cursor_row
            .saturating_sub(layout.list_rows.saturating_sub(1) / 2)
            .min(entries.len().saturating_sub(layout.list_rows));
        let end = (start + layout.list_rows).min(entries.len());
        let preview_width = layout.detail_width.max(1);
        let preview = preview_lines(templates[self.choices[self.cursor]])
            .into_iter()
            .flat_map(|line| {
                wrap_line(&line.text, preview_width)
                    .into_iter()
                    .map(move |text| PreviewLine {
                        text,
                        style: line.style,
                    })
            })
            .collect::<Vec<_>>();
        let max_scroll = if layout.detail_rows > 0 {
            preview.len().saturating_sub(layout.detail_rows)
        } else {
            0
        };
        self.preview_scroll = self.preview_scroll.min(max_scroll);
        let sticky_heading_index = (0..=self.preview_scroll)
            .rev()
            .find(|index| matches!(preview[*index].style, PreviewStyle::Section));

        let colors = colors_enabled();
        let mut stdout = io::stdout();
        execute!(
            stdout,
            cursor::MoveTo(0, terminal_row(layout.top)),
            Clear(ClearType::FromCursorDown),
            cursor::Hide
        )?;

        queue!(
            stdout,
            cursor::MoveTo(0, terminal_row(layout.top)),
            Clear(ClearType::CurrentLine)
        )?;
        set_style(&mut stdout, colors, palette::TITLE)?;
        let title = format!(
            "{}{}{}{}{}{}",
            text::APP_NAME,
            text::TITLE_SEPARATOR,
            if self.multiple {
                text::CHOOSE_FILES
            } else {
                text::CHOOSE_TEMPLATE
            },
            text::TITLE_COUNT_SEPARATOR,
            self.selected.iter().filter(|selected| **selected).count(),
            text::SELECTED_SUFFIX
        );
        write!(stdout, "{}", truncate(&title, width))?;
        reset_style(&mut stdout, colors)?;

        if (self.search_mode || !self.query.is_empty()) && layout.height > 1 {
            queue!(
                stdout,
                cursor::MoveTo(0, terminal_row(layout.top + 1)),
                Clear(ClearType::CurrentLine)
            )?;
            set_style(&mut stdout, colors, palette::MUTED_HEADING)?;
            write!(
                stdout,
                "{}{}{}",
                text::FILTER_PROMPT,
                self.query.escape_debug(),
                if self.search_mode { "▏" } else { "" }
            )?;
            reset_style(&mut stdout, colors)?;
        }

        for (offset, entry) in entries.iter().take(end).skip(start).enumerate() {
            let y = layout.list_top + offset;
            match entry {
                PickerEntry::Category(category) => {
                    draw_category(
                        &mut stdout,
                        colors,
                        layout.list_x,
                        y,
                        &category.escape_debug().to_string(),
                        layout.list_width,
                    )?;
                }
                PickerEntry::Blank => {
                    queue!(
                        stdout,
                        cursor::MoveTo(terminal_row(layout.list_x), terminal_row(y)),
                        Clear(ClearType::CurrentLine)
                    )?;
                }
                PickerEntry::NoResults => {
                    queue!(
                        stdout,
                        cursor::MoveTo(terminal_row(layout.list_x), terminal_row(y)),
                        Clear(ClearType::CurrentLine)
                    )?;
                    set_style(&mut stdout, colors, palette::MUTED_HEADING)?;
                    write!(
                        stdout,
                        "{}{}",
                        text::INDENT,
                        truncate(text::NO_MATCHES, layout.list_width.saturating_sub(2))
                    )?;
                    reset_style(&mut stdout, colors)?;
                }
                PickerEntry::Template(index, row) => {
                    queue!(
                        stdout,
                        cursor::MoveTo(terminal_row(layout.list_x), terminal_row(y)),
                        Clear(ClearType::CurrentLine)
                    )?;
                    let active = *index == self.cursor;
                    set_style(
                        &mut stdout,
                        colors,
                        if active {
                            palette::ACTIVE_TEMPLATE
                        } else {
                            palette::INACTIVE_TEMPLATE
                        },
                    )?;
                    write!(
                        stdout,
                        "{}{}{}",
                        text::INDENT,
                        if active { text::CURSOR } else { text::SPACE },
                        text::SPACE
                    )?;
                    let row_selected = row
                        .alternatives
                        .iter()
                        .any(|template| self.selected[*template]);
                    set_style(
                        &mut stdout,
                        colors,
                        if row_selected {
                            palette::SELECTED_MARKER
                        } else {
                            palette::UNSELECTED_MARKER
                        },
                    )?;
                    write!(
                        stdout,
                        "{}{}",
                        if row_selected {
                            text::SELECTED_MARK
                        } else {
                            text::UNSELECTED_MARK
                        },
                        text::SPACE
                    )?;
                    reset_style(&mut stdout, colors)?;
                    draw_template_choices(
                        &mut stdout,
                        colors,
                        row,
                        templates,
                        self.choices[*index],
                        active,
                        layout.list_width.saturating_sub(6),
                    )?;
                }
            }
        }

        if layout.detail_rows > 0
            && let Some(index) = sticky_heading_index
        {
            let clear = if layout.detail_x == 0 {
                ClearType::CurrentLine
            } else {
                ClearType::UntilNewLine
            };
            queue!(
                stdout,
                cursor::MoveTo(
                    terminal_row(layout.detail_x),
                    terminal_row(layout.detail_top)
                ),
                Clear(clear)
            )?;
            write_preview_line(&mut stdout, colors, &preview[index], layout.detail_width)?;
            reset_style(&mut stdout, colors)?;
        }

        for (offset, (_, line)) in preview
            .iter()
            .enumerate()
            .skip(self.preview_scroll)
            .filter(|(index, _)| Some(*index) != sticky_heading_index)
            .take(layout.detail_rows.saturating_sub(1))
            .enumerate()
        {
            let y = layout.detail_top + 1 + offset;
            let clear = if layout.detail_x == 0 {
                ClearType::CurrentLine
            } else {
                ClearType::UntilNewLine
            };
            queue!(
                stdout,
                cursor::MoveTo(terminal_row(layout.detail_x), terminal_row(y)),
                Clear(clear)
            )?;
            write_preview_line(&mut stdout, colors, line, layout.detail_width)?;
            reset_style(&mut stdout, colors)?;
        }

        queue!(
            stdout,
            cursor::MoveTo(0, terminal_row(layout.footer_y)),
            Clear(ClearType::CurrentLine)
        )?;
        if let Some(notice) = self.notice.as_deref() {
            set_style(&mut stdout, colors, palette::NOTICE)?;
            write!(stdout, "{}", truncate(notice, width))?;
        } else {
            let mut remaining = width;
            let mut footer = if self.search_mode {
                text::FOOTER_SEARCH.to_vec()
            } else if self.multiple {
                text::FOOTER_MULTI.to_vec()
            } else {
                text::FOOTER_SINGLE.to_vec()
            };
            if !self.search_mode && rows[self.cursor].alternatives.len() > 1 {
                footer.insert(1, text::FOOTER_VARIANT);
            }
            if !self.search_mode
                && !self.query.is_empty()
                && let Some(last) = footer.last_mut()
            {
                *last = text::FOOTER_CLEAR_FILTER;
            }
            for (key, hint) in &footer {
                set_style(&mut stdout, colors, palette::FOOTER_KEY)?;
                let visible_key = truncate(key, remaining);
                remaining = remaining.saturating_sub(UnicodeWidthStr::width(visible_key.as_str()));
                write!(stdout, "{visible_key}")?;
                if UnicodeWidthStr::width(visible_key.as_str()) < UnicodeWidthStr::width(*key)
                    || remaining == 0
                {
                    break;
                }

                set_style(&mut stdout, colors, palette::FOOTER_TEXT)?;
                let visible_hint = truncate(hint, remaining);
                remaining = remaining.saturating_sub(UnicodeWidthStr::width(visible_hint.as_str()));
                write!(stdout, "{visible_hint}")?;
                if UnicodeWidthStr::width(visible_hint.as_str()) < UnicodeWidthStr::width(*hint) {
                    break;
                }
            }
        }
        reset_style(&mut stdout, colors)?;
        stdout.flush()?;
        Ok(())
    }
}

fn picker_entries<'a>(rows: &'a [TemplateRow], visible_rows: &[usize]) -> Vec<PickerEntry<'a>> {
    if visible_rows.is_empty() {
        return vec![PickerEntry::NoResults];
    }
    let mut groups: Vec<(&str, Vec<(usize, &TemplateRow)>)> = Vec::new();
    for index in visible_rows {
        let row = &rows[*index];
        if let Some((_, group)) = groups
            .iter_mut()
            .find(|(category, _)| *category == row.category)
        {
            group.push((*index, row));
        } else {
            groups.push((&row.category, vec![(*index, row)]));
        }
    }

    let mut entries = Vec::with_capacity(rows.len() + groups.len() * 2);
    for (index, (category, rows)) in groups.into_iter().enumerate() {
        if index > 0 {
            entries.push(PickerEntry::Blank);
        }
        entries.push(PickerEntry::Category(category));
        entries.extend(
            rows.into_iter()
                .map(|(index, row)| PickerEntry::Template(index, row)),
        );
    }
    entries
}

fn draw_template_choices(
    stdout: &mut impl Write,
    colors: bool,
    row: &TemplateRow,
    templates: &[&Template],
    choice: usize,
    row_active: bool,
    width: usize,
) -> io::Result<()> {
    let mut alternatives = Vec::with_capacity(row.alternatives.len());
    alternatives.push(choice);
    alternatives.extend(
        row.alternatives
            .iter()
            .copied()
            .filter(|index| *index != choice),
    );

    let mut remaining = width;
    for (position, index) in alternatives.into_iter().enumerate() {
        if position > 0 {
            let separator = truncate(text::ALTERNATIVE_SEPARATOR, remaining);
            write!(stdout, "{separator}")?;
            remaining = remaining.saturating_sub(UnicodeWidthStr::width(separator.as_str()));
        }
        if remaining == 0 {
            break;
        }

        let active = index == choice;
        set_style(
            stdout,
            colors,
            if active && (row_active || row.alternatives.len() > 1) {
                palette::ACTIVE_TEMPLATE
            } else {
                palette::INACTIVE_TEMPLATE
            },
        )?;
        let label = templates[index].id.escape_debug().to_string();
        let label = if active && row.alternatives.len() > 1 {
            format!(
                "{}{label}{}",
                text::ALTERNATIVE_PREFIX,
                text::ALTERNATIVE_SUFFIX
            )
        } else {
            label
        };
        let visible = truncate(&label, remaining);
        write!(stdout, "{visible}")?;
        remaining = remaining.saturating_sub(UnicodeWidthStr::width(visible.as_str()));
        reset_style(stdout, colors)?;
    }
    Ok(())
}

pub(super) struct TerminalSession {
    pub(super) top: u16,
    pub(super) rows: usize,
}

impl TerminalSession {
    pub(super) fn enter() -> Result<Self> {
        let (_, height) = terminal::size()?;
        let rows = usize::from(height.saturating_sub(layout::TERMINAL_ROW_RESERVE))
            .clamp(1, layout::MAX_TERMINAL_ROWS);
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        let setup = (|| -> io::Result<u16> {
            stdout.write_all(b"\r\n")?;
            for _ in 0..rows {
                stdout.write_all(b"\r\n")?;
            }
            execute!(stdout, cursor::MoveUp(terminal_row(rows)), cursor::Hide)?;
            stdout.flush()?;
            cursor::position().map(|(_, row)| row)
        })();
        match setup {
            Ok(top) => Ok(Self { top, rows }),
            Err(error) => {
                let _ = execute!(stdout, cursor::Show);
                let _ = terminal::disable_raw_mode();
                Err(error.into())
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        if let Ok((_, height)) = terminal::size() {
            let top = self.top.min(height.saturating_sub(1));
            let _ = execute!(
                stdout,
                cursor::MoveTo(0, top),
                Clear(ClearType::FromCursorDown),
                cursor::MoveTo(0, top),
                cursor::Show
            );
        } else {
            let _ = execute!(stdout, cursor::Show);
        }
        if colors_enabled() {
            let _ = execute!(stdout, style::ResetColor);
        }
        let _ = terminal::disable_raw_mode();
    }
}

fn draw_category(
    stdout: &mut impl Write,
    colors: bool,
    x: usize,
    y: usize,
    text: &str,
    width: usize,
) -> io::Result<()> {
    let clear = if x == 0 {
        ClearType::CurrentLine
    } else {
        ClearType::UntilNewLine
    };
    queue!(
        stdout,
        cursor::MoveTo(terminal_row(x), terminal_row(y)),
        Clear(clear)
    )?;
    set_style(stdout, colors, palette::MUTED_HEADING)?;
    write!(
        stdout,
        "{}{}",
        text::INDENT,
        truncate(text, width.saturating_sub(2))
    )?;
    reset_style(stdout, colors)
}
