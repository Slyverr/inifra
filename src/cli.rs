use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Add reusable starter files to a project")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List the bundled starter templates
    List,
    /// Generate one bundled template in a project directory
    Generate {
        /// Template ID to generate
        template: String,
        /// Project directory to write into
        #[arg(short, long, value_name = "DIR", default_value = ".")]
        path: PathBuf,
        /// Show planned changes without writing files
        #[arg(long)]
        dry_run: bool,
        /// Replace existing files
        #[arg(long)]
        overwrite: bool,
    },
}

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(command) => crate::commands::run(command),
        None => {
            let mut command = Cli::command();
            command.print_help().context("could not print help")?;
            println!();
            Ok(())
        }
    }
}
