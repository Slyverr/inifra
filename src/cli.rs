use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::engine::ArgValueCompleter;

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
        #[arg(add = ArgValueCompleter::new(crate::completions::template_candidates))]
        template: String,
        /// Project directory to write into
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
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
        #[arg(
            long = "template",
            value_name = "ID",
            conflicts_with = "preset",
            add = ArgValueCompleter::new(crate::completions::template_candidates)
        )]
        templates: Vec<String>,
        /// Apply a saved user preset
        #[arg(
            long,
            value_name = "NAME",
            conflicts_with = "templates",
            add = ArgValueCompleter::new(crate::completions::preset_candidates)
        )]
        preset: Option<String>,
        /// Project directory to write into
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
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
    /// List and save reusable template selections
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
        /// Install Fish completions in the user config directory
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PresetCommand {
    /// List saved presets
    List,
    /// Save a template selection as a user preset
    Save {
        /// Preset name
        name: String,
        /// Optional preset description
        #[arg(long, default_value = "")]
        description: String,
        /// Template ID to include (repeatable)
        #[arg(
            long = "template",
            value_name = "ID",
            add = ArgValueCompleter::new(crate::completions::template_candidates)
        )]
        templates: Vec<String>,
        /// Store a shared template input as KEY=VALUE (repeatable)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
        /// Replace an existing preset
        #[arg(long)]
        overwrite: bool,
    },
}

pub(crate) fn run() -> Result<()> {
    clap_complete::CompleteEnv::with_factory(command).complete();
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

pub(crate) fn command() -> clap::Command {
    Cli::command()
}
