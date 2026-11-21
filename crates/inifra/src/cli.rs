use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

use crate::{
    assistant::{self, AskArgs},
    commands::{
        generate::GenerateArgs, init::InitArgs, inspect::InspectArgs, list::ListArgs,
        validate::ValidateArgs,
    },
    completions::CompletionsArgs,
    presets::PresetCommand,
    sources::SourceCommand,
    templates::{EditArgs, RemoveArgs, RenameArgs},
};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Create project files from reusable templates",
    color = clap::ColorChoice::Auto,
    styles = crate::cli_style::styles()
)]
pub(crate) struct Cli {
    /// Path to a template catalog.
    #[arg(long, global = true)]
    pub(crate) catalog: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
#[command(styles = crate::cli_style::styles())]
pub(crate) enum Command {
    /// List available template packs.
    List(ListArgs),
    /// Show details and outputs for a pack.
    Inspect(InspectArgs),
    /// Validate catalog manifests and source paths.
    Validate(ValidateArgs),
    /// Interactively choose and create project files.
    Init(InitArgs),
    /// Copy a bundled template to user config and open it in the configured editor.
    Edit(EditArgs),
    /// Rename a user-owned template and update its references.
    Rename(RenameArgs),
    /// Remove a user-owned template that no saved data references.
    Remove(RemoveArgs),
    /// Manage saved template selections.
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    /// Configure assistant profiles and credentials.
    Assistant {
        #[command(subcommand)]
        command: Option<assistant::AssistantCommand>,
    },
    /// Open an inline assistant session for this project.
    Ask(AskArgs),
    /// Manage project-pinned Git template sources.
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    /// Generate or install shell completions.
    Completions(CompletionsArgs),
    /// Generate files from selected template packs.
    Generate(GenerateArgs),
}

impl Command {
    pub(crate) fn requires_user_config(&self) -> bool {
        matches!(
            self,
            Self::Edit(_) | Self::Rename(_) | Self::Remove(_) | Self::Preset { .. }
        )
    }
}

pub(crate) fn command() -> clap::Command {
    Cli::command()
}
