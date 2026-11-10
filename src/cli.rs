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
    /// List available starter templates
    List,
    /// Generate one template in a project directory
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
        /// Set a shared template input as KEY=VALUE (repeatable)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
    },
    /// Initialize a project with one or more templates
    Init {
        /// Template ID to include (repeatable)
        #[arg(long = "template", value_name = "ID")]
        templates: Vec<String>,
        /// Project directory to write into
        #[arg(short, long, value_name = "DIR", default_value = ".")]
        path: PathBuf,
        /// Show planned changes without writing files
        #[arg(long)]
        dry_run: bool,
        /// Replace existing files
        #[arg(long)]
        overwrite: bool,
        /// Set a shared template input as KEY=VALUE (repeatable)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
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
