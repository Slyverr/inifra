mod generate;
mod list;

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
        } => generate::run(&template, &path, dry_run, overwrite),
    }
}
