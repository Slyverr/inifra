mod completions;
mod edit;
mod generate;
mod list;
mod picker;
mod preset;
mod setup;

use anyhow::Result;

use crate::cli::Command;

pub(crate) fn run(command: Command) -> Result<()> {
    match command {
        Command::List => list::run(),
        Command::Generate {
            template,
            path,
            dry_run,
            overwrite,
            edit,
            inputs,
        } => generate::run(&template, &path, dry_run, overwrite, edit, &inputs),
        Command::Init {
            templates,
            preset,
            path,
            dry_run,
            overwrite,
            edit,
            inputs,
        } => setup::run(
            &templates,
            preset.as_deref(),
            &path,
            dry_run,
            overwrite,
            edit,
            &inputs,
        ),
        Command::Preset { command } => preset::run(command),
        Command::Edit { template } => edit::run(&template),
        Command::Completions { shell, install } => completions::run(shell, install),
    }
}
