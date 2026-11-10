use std::fmt::Display;

use clap::builder::styling::{AnsiColor, Styles};
use console::Style;

#[derive(Clone, Copy)]
pub(crate) enum Tone {
    Accent,
    Muted,
    Success,
    Warning,
    Error,
    Path,
}

pub(crate) const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().bold())
    .error(AnsiColor::Red.on_default().bold())
    .usage(AnsiColor::BrightCyan.on_default().bold())
    .literal(AnsiColor::Green.on_default().bold())
    .placeholder(AnsiColor::Yellow.on_default())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::BrightYellow.on_default())
    .context(AnsiColor::BrightBlack.on_default())
    .context_value(AnsiColor::Yellow.on_default());

pub(crate) fn paint_stdout(text: impl Display, tone: Tone, bold: bool) -> String {
    paint(text, tone, bold, false)
}

pub(crate) fn paint_stderr(text: impl Display, tone: Tone, bold: bool) -> String {
    paint(text, tone, bold, true)
}

fn paint(text: impl Display, tone: Tone, bold: bool, stderr: bool) -> String {
    let style = match tone {
        Tone::Accent => Style::new().cyan(),
        Tone::Muted => Style::new().dim(),
        Tone::Success | Tone::Path => Style::new().green(),
        Tone::Warning => Style::new().yellow(),
        Tone::Error => Style::new().red(),
    };
    let style = if stderr {
        style.for_stderr()
    } else {
        style.for_stdout()
    };
    let colors_enabled = std::env::var_os("NO_COLOR").is_none()
        && if stderr {
            console::colors_enabled_stderr()
        } else {
            console::colors_enabled()
        };
    let style = style.force_styling(colors_enabled);
    let style = if bold { style.bold() } else { style };
    style.apply_to(text).to_string()
}
