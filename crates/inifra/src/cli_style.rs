//! Shared Clap help and error styles backed by Inifra's semantic terminal palette.

use clap::builder::styling::Styles;
use inifra_tui::presentation::{self, Tone};

pub(crate) fn styles() -> Styles {
    let accent = presentation::ansi_color(Tone::Accent).on_default().bold();
    let muted = presentation::ansi_color(Tone::Muted).on_default();
    let warning = presentation::ansi_color(Tone::Warning).on_default();
    let error = presentation::ansi_color(Tone::Error).on_default().bold();
    let success = presentation::ansi_color(Tone::Success).on_default().bold();

    Styles::styled()
        .header(accent)
        .usage(accent)
        .literal(accent)
        .placeholder(warning)
        .valid(success)
        .invalid(warning.bold())
        .error(error)
        .context(muted)
        .context_value(warning)
}

#[cfg(test)]
mod tests {
    use crate::cli::Cli;
    use clap::{ColorChoice, CommandFactory};

    #[test]
    fn root_and_nested_help_use_the_shared_styling() {
        let mut root = Cli::command().color(ColorChoice::Always);
        let root_help = root.render_help().ansi().to_string();
        assert!(root_help.contains("\u{1b}["));

        for name in ["init", "generate", "assistant", "preset", "source"] {
            let nested = root.find_subcommand_mut(name).unwrap().clone();
            let help = nested
                .color(ColorChoice::Always)
                .render_help()
                .ansi()
                .to_string();
            assert!(help.contains("\u{1b}["), "{name} help was not styled");
        }

        let assistant = root.find_subcommand_mut("assistant").unwrap();
        let configure = assistant
            .find_subcommand_mut("configure")
            .unwrap()
            .clone()
            .color(ColorChoice::Always)
            .render_help()
            .ansi()
            .to_string();
        assert!(
            configure.contains("\u{1b}["),
            "nested assistant help was not styled"
        );

        let mut invalid = Cli::command().color(ColorChoice::Always);
        let error = invalid
            .try_get_matches_from_mut(["inifra", "--unknown"])
            .unwrap_err();
        assert!(error.render().ansi().to_string().contains("\u{1b}["));

        let plain = Cli::command()
            .color(ColorChoice::Never)
            .render_help()
            .to_string();
        assert!(!plain.contains("\u{1b}["));
    }
}
