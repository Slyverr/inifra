//! Shared terminal colors and safe text formatting for CLI and inline TUI output.

use anstyle::{AnsiColor, Effects, Style};
use ratatui::style::{Color, Modifier, Style as TuiStyle};
use std::io::{self, IsTerminal};

/// A semantic terminal color used by Inifra's command and interactive output.
#[derive(Clone, Copy)]
pub enum Tone {
    /// Primary emphasis and keyboard guidance.
    Accent,
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

    let color = match tone {
        Tone::Accent => AnsiColor::Cyan,
        Tone::Muted => AnsiColor::BrightBlack,
        Tone::Success => AnsiColor::Green,
        Tone::Warning => AnsiColor::Yellow,
        Tone::Error => AnsiColor::Red,
        Tone::Template => AnsiColor::Magenta,
    };
    let mut style = Style::new().fg_color(Some(color.into()));
    if bold {
        style = style.effects(Effects::BOLD);
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}

/// Returns the Ratatui equivalent of a semantic color, also respecting `NO_COLOR`.
pub(crate) fn tui_style(tone: Tone, bold: bool) -> TuiStyle {
    let color = if std::env::var_os("NO_COLOR").is_some() {
        Color::Reset
    } else {
        match tone {
            Tone::Accent => Color::Cyan,
            Tone::Muted => Color::Gray,
            Tone::Success => Color::Green,
            Tone::Warning => Color::Yellow,
            Tone::Error => Color::Red,
            Tone::Template => Color::Magenta,
        }
    };
    let mut style = TuiStyle::default().fg(color);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
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
    use super::{Tone, paint_with_style, terminal_text};

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
}
