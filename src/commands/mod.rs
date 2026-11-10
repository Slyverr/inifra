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
            inputs,
        } => generate::run(&template, &path, dry_run, overwrite, &inputs),
        Command::Init {
            templates,
            preset,
            path,
            dry_run,
            overwrite,
            inputs,
        } => setup::run(
            &templates,
            preset.as_deref(),
            &path,
            dry_run,
            overwrite,
            &inputs,
        ),
        Command::Preset { command } => preset::run(command),
    }
}
