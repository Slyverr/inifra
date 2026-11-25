//! Shared terminal colors and safe text formatting for CLI and inline TUI output.

use anstyle::{AnsiColor, Effects, RgbColor, Style};
use ratatui::style::{Color, Modifier, Style as TuiStyle};
use std::io::{self, IsTerminal};
use unicode_width::UnicodeWidthStr;

const SECONDARY_RGB: (u8, u8, u8) = (160, 172, 193);

/// A semantic terminal color used by Inifra's command and interactive output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tone {
    /// Primary emphasis and keyboard guidance.
    Accent,
    /// Secondary content that should recede without becoming hard to read.
    Secondary,
    /// Secondary descriptions and paths.
    Muted,
    /// Successful actions.
    Success,
    /// Replacements and other cautionary actions.
    Warning,
    /// Errors and invalid state.
    Error,
    /// Template-management actions and identifiers.
    Template,
    /// File and directory paths.
    Path,
}

/// Paints text only when the selected output stream is a terminal and `NO_COLOR` is unset.
pub fn paint(text: impl AsRef<str>, tone: Tone, stderr: bool) -> String {
    let is_terminal = if stderr {
        io::stderr().is_terminal()
    } else {
        io::stdout().is_terminal()
    };

    let enabled = is_terminal && std::env::var_os("NO_COLOR").is_none();
    paint_with_style(text.as_ref(), tone, false, enabled)
}

/// Paints emphasized text for terminal output, respecting `NO_COLOR` and redirection.
pub fn paint_emphasis(text: impl AsRef<str>, tone: Tone) -> String {
    let enabled = io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    paint_with_style(text.as_ref(), tone, true, enabled)
}

fn paint_with_style(text: &str, tone: Tone, bold: bool, enabled: bool) -> String {
    if !enabled {
        return text.to_owned();
    }

    let mut style = Style::new().fg_color(Some(terminal_color(tone)));
    if bold {
        style = style.effects(Effects::BOLD);
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}

fn terminal_color(tone: Tone) -> anstyle::Color {
    match tone {
        Tone::Secondary => RgbColor(SECONDARY_RGB.0, SECONDARY_RGB.1, SECONDARY_RGB.2).into(),
        _ => ansi_color(tone).into(),
    }
}

/// Returns the ANSI color for a semantic role so CLI integrations can share this palette.
pub fn ansi_color(tone: Tone) -> AnsiColor {
    match tone {
        Tone::Accent => AnsiColor::Cyan,
        Tone::Secondary => AnsiColor::BrightBlack,
        Tone::Muted => AnsiColor::BrightBlack,
        Tone::Success | Tone::Path => AnsiColor::Green,
        Tone::Warning => AnsiColor::Yellow,
        Tone::Error => AnsiColor::Red,
        Tone::Template => AnsiColor::Magenta,
    }
}

/// Returns the active terminal width when stdout is interactive.
pub fn terminal_width() -> Option<usize> {
    if !io::stdout().is_terminal() {
        return None;
    }

    crossterm::terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
        .filter(|columns| *columns > 0)
}

/// Measures text using terminal columns rather than UTF-8 bytes.
pub fn visible_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Truncates text to a terminal-column limit and marks the omitted tail with an ellipsis.
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    if visible_width(text) <= max_width {
        return text.to_owned();
    }

    if max_width == 0 {
        return String::new();
    }

    let content_width = max_width.saturating_sub(1);
    let mut end = 0;
    for (index, character) in text.char_indices() {
        let next = index + character.len_utf8();
        if visible_width(&text[..next]) > content_width {
            break;
        }
        end = next;
    }
    format!("{}…", &text[..end])
}

/// Returns the Ratatui equivalent of a semantic color, also respecting `NO_COLOR`.
pub(crate) fn tui_style(tone: Tone, bold: bool) -> TuiStyle {
    let color = if std::env::var_os("NO_COLOR").is_some() {
        Color::Reset
    } else {
        tui_color(tone)
    };

    let mut style = TuiStyle::default().fg(color);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

/// Returns the dimmed terminal-default style used for section headings.
pub(crate) fn tui_dimmed_style() -> TuiStyle {
    TuiStyle::default().add_modifier(Modifier::DIM)
}

/// Returns the subtle background that sets the user's own messages apart, or no styling when
/// `NO_COLOR` is set.
pub(crate) fn tui_surface_style() -> TuiStyle {
    if std::env::var_os("NO_COLOR").is_some() {
        TuiStyle::default()
    } else {
        TuiStyle::default().bg(Color::Rgb(36, 42, 56))
    }
}

fn tui_color(tone: Tone) -> Color {
    match tone {
        Tone::Accent => Color::Cyan,
        Tone::Secondary => Color::Rgb(SECONDARY_RGB.0, SECONDARY_RGB.1, SECONDARY_RGB.2),
        Tone::Muted => Color::Gray,
        Tone::Success => Color::Green,
        Tone::Warning => Color::Yellow,
        Tone::Error => Color::Red,
        Tone::Template => Color::Magenta,
        Tone::Path => Color::Green,
    }
}

/// Escapes control characters in user-controlled text before it is shown in a terminal.
pub fn terminal_text(text: &str) -> String {
    let mut safe = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            safe.extend(character.escape_debug());
        } else {
            safe.push(character);
        }
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::{
        Tone, ansi_color, paint_with_style, terminal_text, truncate_to_width, tui_color,
        visible_width,
    };
    use ratatui::style::Color;

    #[test]
    fn disabled_color_returns_plain_text() {
        assert_eq!(
            paint_with_style("template", Tone::Accent, true, false),
            "template"
        );
    }

    #[test]
    fn enabled_color_uses_the_shared_palette() {
        let output = paint_with_style("template", Tone::Accent, true, true);
        assert!(output.contains("\u{1b}["));
        assert!(output.contains("template"));
    }

    #[test]
    fn terminal_text_escapes_control_characters() {
        assert_eq!(terminal_text("row\n\u{1b}[31m"), "row\\n\\u{1b}[31m");
    }

    #[test]
    fn muted_tui_text_uses_the_terminal_gray_color() {
        assert_eq!(tui_color(Tone::Muted), Color::Gray);
    }

    #[test]
    fn path_tone_uses_the_shared_green_palette() {
        assert_eq!(ansi_color(Tone::Path), anstyle::AnsiColor::Green);
        assert_eq!(tui_color(Tone::Path), Color::Green);
    }

    #[test]
    fn truncation_uses_display_width_and_an_ellipsis() {
        let truncated = truncate_to_width("readme 日本語", 10);
        assert!(truncated.ends_with('…'));
        assert!(visible_width(&truncated) <= 10);
        assert_eq!(truncate_to_width("short", 10), "short");
    }
}
