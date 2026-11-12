mod completions;
mod edit;
mod generate;
mod list;
mod picker;
mod preset;
mod setup;
mod source;
mod validate;

use anyhow::Result;

use crate::cli::Command;

pub(crate) fn run(command: Command) -> Result<()> {
    match command {
        Command::List { path } => list::run(path.as_deref()),
        Command::Validate { template, path } => validate::run(template.as_deref(), path.as_deref()),
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
        Command::Source { command } => source::run(command),
        Command::Edit { template, path } => edit::run(&template, path.as_deref()),
        Command::Completions { shell, install } => completions::run(shell, install),
    }
}
