mod assistant_cli;
mod assistant_configure;
mod completions;
mod init;
mod preset_cli;
mod source_cache;
mod source_cli;
mod templates;
mod user_config;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use inifra_core::{apply, workflow};
use inifra_tui::presentation;
use std::{collections::BTreeMap, error::Error, path::PathBuf};

#[derive(Parser)]
#[command(version, about = "Create project files from reusable templates")]
struct Cli {
    /// Path to a template catalog.
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List available template packs.
    List {
        /// Show every file and directory output for each pack.
        #[arg(long)]
        files: bool,
    },
    /// Show details and outputs for a pack.
    Inspect {
        #[arg(add = completions::template_completer())]
        id: String,
    },
    /// Validate catalog manifests and source paths.
    Validate,
    /// Interactively choose and create project files.
    Init {
        /// Destination directory.
        #[arg(long, default_value = ".")]
        target: PathBuf,
        /// Replace existing files after approval.
        #[arg(long)]
        overwrite: bool,
    },
    /// Copy a bundled template to user config and open it in the configured editor.
    Edit {
        #[arg(add = completions::template_completer())]
        id: String,
    },
    /// Rename a user-owned template and update its references.
    Rename {
        #[arg(add = completions::template_completer())]
        old_id: String,
        new_id: String,
    },
    /// Remove a user-owned template that no saved data references.
    Remove {
        #[arg(add = completions::template_completer())]
        id: String,
    },
    /// Manage saved template selections.
    Preset {
        #[command(subcommand)]
        command: preset_cli::PresetCommand,
    },
    /// Configure assistant profiles and credentials.
    Assistant {
        #[command(subcommand)]
        command: Option<assistant_cli::AssistantCommand>,
    },
    /// Manage project-pinned Git template sources.
    Source {
        #[command(subcommand)]
        command: source_cli::SourceCommand,
    },
    /// Generate or install shell completions.
    Completions {
        /// Shell that should receive completions.
        #[arg(value_enum)]
        shell: Shell,
        /// Install completions in the shell's user completion directory.
        #[arg(long, conflicts_with = "uninstall")]
        install: bool,
        /// Remove Inifra's generated completion file.
        #[arg(long)]
        uninstall: bool,
    },
    /// Generate files from selected template packs.
    Generate {
        #[arg(required = true, value_name = "ID", add = completions::template_completer())]
        ids: Vec<String>,
        #[arg(long, default_value = ".")]
        target: PathBuf,
        #[arg(
            long,
            value_name = "NAME=VALUE",
            add = completions::input_completer()
        )]
        set: Vec<String>,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            presentation::paint(
                format!("error: {}", presentation::terminal_text(&error.to_string())),
                presentation::Tone::Error,
                true
            )
        );
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    clap_complete::env::CompleteEnv::with_factory(Cli::command)
        .var(completions::COMPLETION_ENV)
        .complete();
    let cli = Cli::parse();
    if let Command::Completions {
        shell,
        install,
        uninstall,
    } = &cli.command
    {
        if cli.catalog.is_some() {
            return Err("--catalog cannot be used with completions".into());
        }
        return completions::run(*shell, *install, *uninstall);
    }
    let command = match cli.command {
        Command::Assistant { command } => {
            if cli.catalog.is_some() {
                return Err("--catalog cannot be used with assistant configuration".into());
            }
            return assistant_cli::run(command, user_config::config_dir()?);
        }
        Command::Source { command } => {
            if cli.catalog.is_some() {
                return Err("--catalog cannot be used with source management".into());
            }
            return source_cli::run(command, &std::env::current_dir()?);
        }
        command => command,
    };
    if cli.catalog.is_some() && command.requires_user_config() {
        return Err("user template and preset commands cannot be used with --catalog".into());
    }
    let config_dir = if cli.catalog.is_none() || command.requires_user_config() {
        user_config::config_dir()?
    } else {
        None
    };
    let project_dir = std::env::current_dir()?;
    let (catalog, _bundled_catalog_dir) = user_config::load_project_catalog(
        cli.catalog.as_deref(),
        config_dir.as_deref(),
        &project_dir,
    )?;
    match command {
        Command::List { files } => {
            let mut category = "";
            let width = catalog
                .packs
                .iter()
                .map(|pack| pack.id.len())
                .max()
                .unwrap_or(0);
            for pack in &catalog.packs {
                if category != pack.category {
                    if !category.is_empty() {
                        println!();
                    }
                    category = &pack.category;
                    println!(
                        "{}",
                        presentation::paint_emphasis(
                            presentation::terminal_text(&catalog.categories[category].name),
                            presentation::Tone::Muted
                        )
                    );
                }
                let selected = std::slice::from_ref(&pack.id);
                let outputs = catalog.resolve_outputs(selected)?;
                let id = presentation::paint_emphasis(
                    format!("{:width$}", presentation::terminal_text(&pack.id)),
                    presentation::Tone::Accent,
                );
                let description = presentation::terminal_text(&pack.description);
                if files {
                    println!("  {id}  {description}");
                    if !outputs.is_empty() {
                        println!(
                            "    {}",
                            presentation::paint_emphasis("Files:", presentation::Tone::Muted)
                        );
                        for output in outputs {
                            println!(
                                "      {}",
                                presentation::paint(
                                    presentation::terminal_text(
                                        &output.output.display().to_string()
                                    ),
                                    presentation::Tone::Muted,
                                    false
                                )
                            );
                        }
                    }
                    if !pack.directories.is_empty() {
                        println!(
                            "    {}",
                            presentation::paint_emphasis("Directories:", presentation::Tone::Muted)
                        );
                        for directory in &pack.directories {
                            println!(
                                "      {}",
                                presentation::paint(
                                    format!(
                                        "{}/",
                                        presentation::terminal_text(
                                            &directory.display().to_string()
                                        )
                                    ),
                                    presentation::Tone::Muted,
                                    false
                                )
                            );
                        }
                    }
                } else {
                    let summary = compact_output_summary(&outputs, &pack.directories);
                    println!(
                        "  {id}  {description}  {}",
                        presentation::paint(
                            presentation::terminal_text(&summary),
                            presentation::Tone::Muted,
                            false
                        )
                    );
                }
            }
        }
        Command::Inspect { id } => {
            let pack = catalog
                .packs
                .iter()
                .find(|pack| pack.id == id)
                .ok_or_else(|| format!("unknown template {id:?}"))?;
            println!(
                "{}",
                presentation::paint_emphasis(
                    presentation::terminal_text(&pack.id),
                    presentation::Tone::Accent
                )
            );
            println!("{}", presentation::terminal_text(&pack.description));
            let output_paths = catalog.resolve_outputs(std::slice::from_ref(&id))?;
            if !output_paths.is_empty() {
                println!();
                println!(
                    "{}",
                    presentation::paint_emphasis("Files:", presentation::Tone::Muted)
                );
                for output in output_paths {
                    println!(
                        "  {}",
                        presentation::paint(
                            presentation::terminal_text(&output.output.display().to_string()),
                            presentation::Tone::Muted,
                            false
                        )
                    );
                    if let Some(alternative) = output.co_selected_output {
                        println!(
                            "    {}",
                            presentation::paint(
                                format!(
                                    "when paired ({})  {}",
                                    presentation::terminal_text(&alternative.group),
                                    presentation::terminal_text(
                                        &alternative.path.display().to_string()
                                    )
                                ),
                                presentation::Tone::Muted,
                                false
                            )
                        );
                    }
                }
            }
            if !pack.directories.is_empty() {
                println!();
                println!(
                    "{}",
                    presentation::paint_emphasis("Directories:", presentation::Tone::Muted)
                );
                for directory in &pack.directories {
                    println!(
                        "  {}",
                        presentation::paint(
                            format!(
                                "{}/",
                                presentation::terminal_text(&directory.display().to_string())
                            ),
                            presentation::Tone::Muted,
                            false
                        )
                    );
                }
            }
            let definitions = catalog.input_definitions_for(pack)?;
            if !definitions.is_empty() {
                println!();
                println!(
                    "{}",
                    presentation::paint_emphasis("Inputs:", presentation::Tone::Muted)
                );
                for (name, definition) in definitions {
                    let requirement = if definition.required {
                        "required"
                    } else {
                        "optional"
                    };
                    println!(
                        "  {} ({requirement})  {}",
                        presentation::terminal_text(&name),
                        presentation::terminal_text(&definition.prompt)
                    );
                    if let Some(default) = &definition.default {
                        println!("    Default: {}", presentation::terminal_text(default));
                    }
                }
            }
        }
        Command::Validate => {
            catalog.validate()?;
            for pack in &catalog.packs {
                println!(
                    "{}  {}",
                    presentation::paint("valid", presentation::Tone::Success, false),
                    presentation::paint_emphasis(&pack.id, presentation::Tone::Accent)
                );
            }
            println!("Validated {} template(s).", catalog.packs.len());
        }
        Command::Init { target, overwrite } => init::run(&catalog, target, overwrite)?,
        Command::Edit { id } => {
            templates::edit(&catalog, config_dir.as_deref(), &project_dir, &id)?
        }
        Command::Rename { old_id, new_id } => templates::rename(
            &catalog,
            config_dir.as_deref(),
            &project_dir,
            &old_id,
            &new_id,
        )?,
        Command::Remove { id } => templates::remove(&catalog, config_dir.as_deref(), &id)?,
        Command::Preset { command } => {
            preset_cli::run(&catalog, config_dir.as_deref(), command)?;
        }
        Command::Assistant { .. } => unreachable!("handled before catalog loading"),
        Command::Source { .. } => unreachable!("handled before catalog loading"),
        Command::Completions { .. } => unreachable!("handled before catalog loading"),
        Command::Generate {
            ids,
            target,
            set,
            overwrite,
            dry_run,
        } => {
            let values = parse_assignments(&set)?;
            let plan = workflow::prepare(&catalog, &ids, &values, &target, overwrite)?;
            if dry_run {
                print_plan(&plan, &target);
                println!(
                    "{}",
                    presentation::paint(
                        "Dry run; no files were changed.",
                        presentation::Tone::Muted,
                        false
                    )
                );
            } else {
                apply::apply_plan(&target, &plan)?;
                println!(
                    "Target: {}",
                    presentation::terminal_text(&target.display().to_string())
                );
                for directory in &plan.directories {
                    if directory.action == inifra_core::plan::DirectoryAction::Create {
                        println!(
                            "{}  {}/",
                            presentation::paint(
                                "created directory",
                                presentation::Tone::Success,
                                false
                            ),
                            presentation::terminal_text(&directory.path.display().to_string())
                        );
                    }
                }
                for file in &plan.files {
                    let (action, tone) = match file.action {
                        inifra_core::plan::Action::Create => {
                            ("created", presentation::Tone::Success)
                        }
                        inifra_core::plan::Action::Replace => {
                            ("replaced", presentation::Tone::Warning)
                        }
                        inifra_core::plan::Action::Skip => {
                            ("already exists", presentation::Tone::Muted)
                        }
                    };
                    println!(
                        "{}  {}",
                        presentation::paint(action, tone, false),
                        presentation::terminal_text(&file.path.display().to_string())
                    );
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn parse_assignments(
    assignments: &[String],
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut values = BTreeMap::new();
    for assignment in assignments {
        let (key, value) = assignment
            .split_once('=')
            .ok_or("--set expects NAME=VALUE")?;
        if values.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("duplicate input {key:?}").into());
        }
    }
    Ok(values)
}

impl Command {
    fn requires_user_config(&self) -> bool {
        matches!(
            self,
            Self::Edit { .. } | Self::Rename { .. } | Self::Remove { .. } | Self::Preset { .. }
        )
    }
}

fn print_plan(plan: &inifra_core::plan::GenerationPlan, target: &std::path::Path) {
    println!(
        "{}: {}",
        presentation::paint_emphasis("Plan for", presentation::Tone::Accent),
        presentation::terminal_text(&target.display().to_string())
    );
    for directory in &plan.directories {
        let (action, tone) = match directory.action {
            inifra_core::plan::DirectoryAction::Create => {
                ("create directory", presentation::Tone::Success)
            }
            inifra_core::plan::DirectoryAction::Existing => {
                ("already exists", presentation::Tone::Muted)
            }
        };
        println!(
            "  {}  {}/",
            presentation::paint(action, tone, false),
            presentation::terminal_text(&directory.path.display().to_string())
        );
    }
    for file in &plan.files {
        let (action, tone) = match file.action {
            inifra_core::plan::Action::Create => ("create", presentation::Tone::Success),
            inifra_core::plan::Action::Replace => ("replace", presentation::Tone::Warning),
            inifra_core::plan::Action::Skip => ("already exists", presentation::Tone::Muted),
        };
        println!(
            "  {}  {}",
            presentation::paint(action, tone, false),
            presentation::terminal_text(&file.path.display().to_string())
        );
    }
}

fn compact_output_summary(
    outputs: &[inifra_core::output_rules::ResolvedPackOutput],
    directories: &[PathBuf],
) -> String {
    let files = outputs.first().map(|first| {
        let remaining = outputs.len() - 1;
        if remaining == 0 {
            first.output.display().to_string()
        } else {
            format!("{} +{remaining} other files", first.output.display())
        }
    });
    let directories = match directories {
        [directory] if outputs.is_empty() => Some(format!("{}/", directory.display())),
        [] => None,
        directories => {
            let suffix = if directories.len() == 1 {
                "directory"
            } else {
                "directories"
            };
            Some(format!("{} {suffix}", directories.len()))
        }
    };

    match (files, directories) {
        (Some(files), Some(directories)) => format!("{files}, {directories}"),
        (Some(files), None) => files,
        (None, Some(directories)) => directories,
        (None, None) => String::new(),
    }
}
