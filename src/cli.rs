use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::engine::ArgValueCompleter;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Add reusable starter files to a project",
    styles = crate::theme::HELP_STYLES
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List available starter templates
    List {
        /// Include templates from this project's .inifra directory
        #[arg(short, long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        path: Option<PathBuf>,
    },
    /// Validate available templates without writing project files
    Validate {
        /// Validate only this template ID
        #[arg(add = ArgValueCompleter::new(crate::completions::template_candidates))]
        template: Option<String>,
        /// Include templates from this project's .inifra directory
        #[arg(short, long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        path: Option<PathBuf>,
    },
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
        /// Open generated text files in the configured editor before writing
        #[arg(long)]
        edit: bool,
        /// Set a shared template input as KEY=VALUE (repeatable)
        #[arg(
            long = "set",
            value_name = "KEY=VALUE",
            add = ArgValueCompleter::new(crate::completions::input_candidates)
        )]
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
        /// Open generated text files in the configured editor before writing
        #[arg(long)]
        edit: bool,
        /// Set a shared template input as KEY=VALUE (repeatable)
        #[arg(
            long = "set",
            value_name = "KEY=VALUE",
            add = ArgValueCompleter::new(crate::completions::input_candidates)
        )]
        inputs: Vec<String>,
    },
    /// List and save reusable template selections
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    /// Manage pinned Git template sources for a project
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    /// Open a local template in the configured editor
    Edit {
        /// Template ID to edit or create
        #[arg(add = ArgValueCompleter::new(crate::completions::template_candidates))]
        template: String,
        /// Project directory containing .inifra/templates
        #[arg(short, long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        path: Option<PathBuf>,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
        /// Install completions in the user config directory without editing shell startup files
        #[arg(long, visible_alias = "apply")]
        install: bool,
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
        #[arg(
            long = "set",
            value_name = "KEY=VALUE",
            add = ArgValueCompleter::new(crate::completions::input_candidates)
        )]
        inputs: Vec<String>,
        /// Project directory whose templates may be saved in the preset
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
        path: PathBuf,
        /// Replace an existing preset
        #[arg(long)]
        overwrite: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SourceCommand {
    /// List the project's pinned template sources
    List {
        /// Project directory containing .inifra/sources.toml
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
        path: PathBuf,
    },
    /// Add a Git source and pin the fetched revision
    Add {
        /// Source name used to qualify template IDs
        name: String,
        /// Git repository URL
        url: String,
        /// Branch, tag, or commit to resolve and pin
        #[arg(long, value_name = "REF")]
        revision: String,
        /// Directory inside the repository containing template packs
        #[arg(long, default_value = "templates", value_name = "DIR")]
        directory: String,
        /// Project directory to update
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
        path: PathBuf,
    },
    /// Fetch missing pinned revisions into the local cache
    Sync {
        /// Project directory whose source registry should be synced
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
        path: PathBuf,
    },
    /// Update a source pin to a newly resolved revision
    Update {
        /// Source name
        name: String,
        /// Branch, tag, or commit to resolve and pin
        #[arg(long, value_name = "REF")]
        revision: String,
        /// Project directory containing the source registry
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
        path: PathBuf,
    },
    /// Remove a source declaration from the project
    Remove {
        /// Source name
        name: String,
        /// Project directory containing the source registry
        #[arg(short, long, value_name = "DIR", default_value = ".", value_hint = ValueHint::DirPath)]
        path: PathBuf,
    },
}

pub(crate) fn run() -> Result<()> {
    clap_complete::CompleteEnv::with_factory(command)
        .bin("inifra")
        .completer("inifra")
        .complete();
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
