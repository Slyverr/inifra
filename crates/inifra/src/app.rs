use std::{error::Error, path::PathBuf};

use clap::Parser;

use crate::{
    assistant,
    cli::{self, Cli, Command},
    commands, completions,
    config::user,
    presets, sources,
    templates::{self, EditArgs, RemoveArgs, RenameArgs},
};

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    clap_complete::env::CompleteEnv::with_factory(cli::command)
        .var(completions::COMPLETION_ENV)
        .complete();

    let cli = Cli::parse();

    match cli.command {
        Command::Completions(args) => {
            reject_catalog(cli.catalog, "completions")?;
            completions::run(
                args.shell,
                args.install,
                args.uninstall,
                args.overwrite,
                args.force,
            )
        }
        Command::Assistant { command } => {
            reject_catalog(cli.catalog, "assistant configuration")?;
            assistant::cli::run(command, user::config_dir()?)
        }
        Command::Ask(args) => assistant::ask::run(
            args.profile.as_deref(),
            cli.catalog.as_deref(),
            user::config_dir()?,
            std::env::current_dir()?,
        ),
        Command::Source { command } => {
            reject_catalog(cli.catalog, "source management")?;
            sources::run(command, &std::env::current_dir()?)
        }
        command => run_catalog_command(command, cli.catalog),
    }
}

fn reject_catalog(catalog: Option<PathBuf>, command: &str) -> Result<(), Box<dyn Error>> {
    if catalog.is_some() {
        return Err(format!("--catalog cannot be used with {command}").into());
    }

    Ok(())
}

fn run_catalog_command(
    command: Command,
    explicit_catalog: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    if explicit_catalog.is_some() && command.requires_user_config() {
        return Err("user template and preset commands cannot be used with --catalog".into());
    }

    let config_dir = if explicit_catalog.is_none() || command.requires_user_config() {
        user::config_dir()?
    } else {
        None
    };

    let project_dir = std::env::current_dir()?;
    let (catalog, _bundled_catalog_dir) = user::load_project_catalog(
        explicit_catalog.as_deref(),
        config_dir.as_deref(),
        &project_dir,
    )?;

    match command {
        Command::List(args) => commands::list::run(&catalog, args),
        Command::Inspect(args) => commands::inspect::run(&catalog, args),
        Command::Validate(args) => commands::validate::run(&catalog, args),
        Command::Init(args) => commands::init::run(&catalog, args.target, args.overwrite),
        Command::Edit(EditArgs { id }) => {
            templates::edit(&catalog, config_dir.as_deref(), &project_dir, &id)
        }
        Command::Rename(RenameArgs { old_id, new_id }) => templates::rename(
            &catalog,
            config_dir.as_deref(),
            &project_dir,
            &old_id,
            &new_id,
        ),
        Command::Remove(RemoveArgs { id }) => {
            templates::remove(&catalog, config_dir.as_deref(), &id)
        }
        Command::Preset { command } => presets::run(&catalog, config_dir.as_deref(), command),
        Command::Generate(args) => commands::generate::run(&catalog, args),
        Command::Assistant { .. }
        | Command::Ask(_)
        | Command::Source { .. }
        | Command::Completions(_) => {
            Err("internal routing error: command does not use the project catalog".into())
        }
    }
}

pub(crate) fn report_error(error: Box<dyn Error>) -> ! {
    eprintln!(
        "{}",
        crate::presentation::paint(
            format!(
                "error: {}",
                crate::presentation::terminal_text(&error.to_string())
            ),
            crate::presentation::Tone::Error,
            true
        )
    );
    std::process::exit(1)
}
