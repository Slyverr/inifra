mod assistant_ask;
mod assistant_cli;
mod assistant_configure;
mod cli_style;
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
use similar::TextDiff;
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Component, Path, PathBuf},
};

const PLAN_ACTION_WIDTH: usize = "skip (already exists)".len();

#[derive(Parser)]
#[command(
    version,
    about = "Create project files from reusable templates",
    color = clap::ColorChoice::Auto,
    styles = crate::cli_style::styles()
)]
struct Cli {
    /// Path to a template catalog.
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
#[command(styles = crate::cli_style::styles())]
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
    /// Open an inline assistant session for this project.
    Ask {
        /// Use a configured profile instead of the default.
        #[arg(long)]
        profile: Option<String>,
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
        /// Replace an existing completion file not marked as Inifra-generated.
        #[arg(long, requires = "install", conflicts_with = "uninstall")]
        overwrite: bool,
        /// Remove an existing completion file not marked as Inifra-generated.
        #[arg(long, requires = "uninstall", conflicts_with = "install")]
        force: bool,
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
        /// Show new content and replacement diffs in a dry run.
        #[arg(long, requires = "dry_run")]
        show_content: bool,
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
        overwrite,
        force,
    } = &cli.command
    {
        if cli.catalog.is_some() {
            return Err("--catalog cannot be used with completions".into());
        }
        return completions::run(*shell, *install, *uninstall, *overwrite, *force);
    }
    let command = match cli.command {
        Command::Ask { profile } => {
            return assistant_ask::run(
                profile.as_deref(),
                cli.catalog.as_deref(),
                user_config::config_dir()?,
                std::env::current_dir()?,
            );
        }
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
            let terminal_width = presentation::terminal_width();
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
                            terminal_width.map_or_else(
                                || presentation::terminal_text(&catalog.categories[category].name),
                                |width| presentation::truncate_to_width(
                                    &presentation::terminal_text(
                                        &catalog.categories[category].name
                                    ),
                                    width
                                )
                            ),
                            presentation::Tone::Muted
                        )
                    );
                }
                let selected = std::slice::from_ref(&pack.id);
                let outputs = catalog.resolve_outputs(selected)?;
                let id = format!("{:width$}", presentation::terminal_text(&pack.id));
                let description = presentation::terminal_text(&pack.description);
                if files {
                    let description = fit_list_description(&id, &description, terminal_width);
                    println!(
                        "  {}  {description}",
                        presentation::paint_emphasis(&id, presentation::Tone::Accent)
                    );
                    for paths in compact_output_paths(&outputs, &pack.directories) {
                        println!(
                            "    {}",
                            presentation::paint(
                                presentation::terminal_text(&paths),
                                presentation::Tone::Muted,
                                false
                            )
                        );
                    }
                } else {
                    let summary = compact_output_summary(&outputs, &pack.directories);
                    let (description, summary) =
                        fit_list_content(&id, &description, &summary, terminal_width);
                    let output_summary = summary.map_or_else(String::new, |summary| {
                        format!(
                            "  {}",
                            presentation::paint(
                                presentation::terminal_text(&summary),
                                presentation::Tone::Muted,
                                false
                            )
                        )
                    });
                    println!(
                        "  {}  {description}{output_summary}",
                        presentation::paint_emphasis(&id, presentation::Tone::Accent)
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
                    presentation::paint_emphasis("Files:", presentation::Tone::Accent)
                );
                for output in output_paths {
                    println!(
                        "  {}",
                        presentation::paint(
                            presentation::terminal_text(&output.output.display().to_string()),
                            presentation::Tone::Path,
                            false
                        )
                    );
                    if let Some(alternative) = output.co_selected_output {
                        println!(
                            "    {}  {}",
                            presentation::paint(
                                format!(
                                    "when paired ({})",
                                    presentation::terminal_text(&alternative.group)
                                ),
                                presentation::Tone::Muted,
                                false
                            ),
                            presentation::paint(
                                presentation::terminal_text(
                                    &alternative.path.display().to_string()
                                ),
                                presentation::Tone::Path,
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
                    presentation::paint_emphasis("Directories:", presentation::Tone::Accent)
                );
                for directory in &pack.directories {
                    println!(
                        "  {}",
                        presentation::paint(
                            format!(
                                "{}/",
                                presentation::terminal_text(&directory.display().to_string())
                            ),
                            presentation::Tone::Path,
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
                    presentation::paint_emphasis("Inputs:", presentation::Tone::Accent)
                );
                for (name, definition) in definitions {
                    let requirement = if definition.required {
                        "required"
                    } else {
                        "optional"
                    };
                    println!(
                        "  {} {}  {}",
                        presentation::paint_emphasis(
                            presentation::terminal_text(&name),
                            presentation::Tone::Warning
                        ),
                        presentation::paint(
                            format!("({requirement})"),
                            presentation::Tone::Muted,
                            false
                        ),
                        presentation::terminal_text(&definition.prompt)
                    );
                    if let Some(default) = &definition.default {
                        println!(
                            "    {} {}",
                            presentation::paint("Default:", presentation::Tone::Muted, false),
                            presentation::paint_emphasis(
                                presentation::terminal_text(default),
                                presentation::Tone::Warning
                            )
                        );
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
            println!(
                "{} {} template(s).",
                presentation::paint("Validated", presentation::Tone::Success, false),
                presentation::paint_emphasis(
                    catalog.packs.len().to_string(),
                    presentation::Tone::Accent
                )
            );
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
        Command::Ask { .. } => unreachable!("handled before catalog loading"),
        Command::Source { .. } => unreachable!("handled before catalog loading"),
        Command::Completions { .. } => unreachable!("handled before catalog loading"),
        Command::Generate {
            ids,
            target,
            set,
            overwrite,
            dry_run,
            show_content,
        } => {
            let values = parse_assignments(&set)?;
            let plan = workflow::prepare(&catalog, &ids, &values, &target, overwrite)?;
            if dry_run {
                print_plan(&plan, &target, show_content);
                println!(
                    "{}",
                    presentation::paint("No files were changed.", presentation::Tone::Muted, false)
                );
            } else {
                apply::apply_plan(&target, &plan)?;
                println!(
                    "{} {}",
                    presentation::paint_emphasis("Target:", presentation::Tone::Accent),
                    presentation::paint(
                        presentation::terminal_text(&target.display().to_string()),
                        presentation::Tone::Path,
                        false
                    )
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
                            presentation::paint(
                                presentation::terminal_text(&directory.path.display().to_string()),
                                presentation::Tone::Path,
                                false
                            )
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
                        presentation::paint(
                            presentation::terminal_text(&file.path.display().to_string()),
                            presentation::Tone::Path,
                            false
                        )
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

fn print_plan(
    plan: &inifra_core::plan::GenerationPlan,
    target: &std::path::Path,
    show_content: bool,
) {
    let target = display_target(target);
    println!(
        "{}  {}",
        presentation::paint_emphasis("Preview", presentation::Tone::Accent),
        presentation::paint(
            presentation::terminal_text(&target.display().to_string()),
            presentation::Tone::Path,
            false
        )
    );
    println!();
    println!(
        "{}",
        presentation::paint_emphasis("Plan", presentation::Tone::Accent)
    );
    for directory in &plan.directories {
        let (action, tone) = match directory.action {
            inifra_core::plan::DirectoryAction::Create => {
                ("create directory", presentation::Tone::Success)
            }
            inifra_core::plan::DirectoryAction::Existing => {
                ("skip (already exists)", presentation::Tone::Muted)
            }
        };
        print_plan_entry(action, tone, &format!("{}/", directory.path.display()));
    }
    for file in &plan.files {
        let (action, tone) = match file.action {
            inifra_core::plan::Action::Create => ("create", presentation::Tone::Success),
            inifra_core::plan::Action::Replace => ("replace", presentation::Tone::Warning),
            inifra_core::plan::Action::Skip => ("skip (already exists)", presentation::Tone::Muted),
        };
        print_plan_entry(action, tone, &file.path.display().to_string());
    }

    if show_content {
        let content_files = plan
            .files
            .iter()
            .filter(|file| file.action != inifra_core::plan::Action::Skip)
            .collect::<Vec<_>>();
        println!();
        println!(
            "{}",
            presentation::paint_emphasis("Content", presentation::Tone::Accent)
        );
        if content_files.is_empty() {
            let message = if plan.files.is_empty() {
                "No file content to preview."
            } else {
                "All existing files are skipped. Pass --overwrite to preview replacements."
            };
            println!(
                "{}",
                presentation::paint(message, presentation::Tone::Muted, false)
            );
        }
        for file in content_files {
            println!();
            match file.action {
                inifra_core::plan::Action::Create => {
                    println!(
                        "{}",
                        presentation::paint(
                            format!(
                                "--- {} ---",
                                presentation::terminal_text(&file.path.display().to_string())
                            ),
                            presentation::Tone::Path,
                            false
                        )
                    );
                    print_rendered_content(&file.path, &file.contents);
                }
                inifra_core::plan::Action::Replace => {
                    if let Some(existing) = file.existing_contents.as_deref() {
                        print_replacement_preview(&file.path, existing, &file.contents);
                    }
                }
                inifra_core::plan::Action::Skip => {}
            }
        }
    }
}

fn print_plan_entry(action: &str, tone: presentation::Tone, path: &str) {
    println!(
        "  {}  {}",
        presentation::paint(format!("{action:<PLAN_ACTION_WIDTH$}"), tone, false),
        presentation::paint_emphasis(
            presentation::terminal_text(path),
            presentation::Tone::Accent
        )
    );
}

fn print_replacement_preview(path: &Path, existing: &[u8], proposed: &[u8]) {
    let (Ok(existing), Ok(proposed)) =
        (std::str::from_utf8(existing), std::str::from_utf8(proposed))
    else {
        println!(
            "{}",
            presentation::paint_emphasis(
                presentation::terminal_text(&path.display().to_string()),
                presentation::Tone::Accent
            )
        );
        println!(
            "  {}",
            presentation::paint(
                format!(
                    "Binary output: {} → {} bytes",
                    existing.len(),
                    proposed.len()
                ),
                presentation::Tone::Muted,
                false
            )
        );
        return;
    };

    if existing == proposed {
        println!(
            "{}  {}",
            presentation::paint_emphasis(
                presentation::terminal_text(&path.display().to_string()),
                presentation::Tone::Accent
            ),
            presentation::paint("no content changes", presentation::Tone::Muted, false)
        );
        return;
    }

    let path = presentation::terminal_text(&path.display().to_string());
    let diff = TextDiff::from_lines(existing, proposed)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string();
    for line in diff.lines() {
        let tone = match line {
            line if line.starts_with("--- ") || line.starts_with("+++ ") => {
                presentation::Tone::Muted
            }
            line if line.starts_with("@@") => presentation::Tone::Accent,
            line if line.starts_with('+') => presentation::Tone::Success,
            line if line.starts_with('-') => presentation::Tone::Error,
            _ => presentation::Tone::Secondary,
        };
        println!(
            "  {}",
            presentation::paint(presentation::terminal_text(line), tone, false)
        );
    }
}

fn display_target(target: &Path) -> PathBuf {
    if let Ok(absolute) = target.canonicalize() {
        return absolute;
    }
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(target))
            .unwrap_or_else(|_| target.to_path_buf())
    }
}

fn print_rendered_content(path: &Path, contents: &[u8]) {
    const INDENT: &str = "  ";

    match std::str::from_utf8(contents) {
        Ok("") => println!("{INDENT}"),
        Ok(content) => {
            let is_markdown = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("md")
                        || extension.eq_ignore_ascii_case("markdown")
                });
            let mut in_code_block = false;
            for line in content.lines() {
                let safe_line = presentation::terminal_text(line);
                let rendered = if is_markdown {
                    color_markdown_line(&safe_line, &mut in_code_block)
                } else {
                    safe_line
                };
                println!("{INDENT}{rendered}");
            }
        }
        Err(_) => println!("{INDENT}[binary output: {} bytes]", contents.len()),
    }
}

fn color_markdown_line(line: &str, in_code_block: &mut bool) -> String {
    let trimmed = line.trim_start();
    let indentation = &line[..line.len() - trimmed.len()];
    if trimmed.starts_with("```") {
        *in_code_block = !*in_code_block;
        return format!(
            "{indentation}{}",
            presentation::paint(trimmed, presentation::Tone::Muted, false)
        );
    }
    if *in_code_block {
        return format!(
            "{indentation}{}",
            presentation::paint(trimmed, presentation::Tone::Secondary, false)
        );
    }
    if trimmed.starts_with('#') {
        return presentation::paint_emphasis(line, presentation::Tone::Warning);
    }
    if trimmed.starts_with("<!--") {
        return presentation::paint(line, presentation::Tone::Muted, false);
    }
    if let Some((marker, rest)) = ["- ", "* ", "+ "]
        .into_iter()
        .find_map(|marker| trimmed.strip_prefix(marker).map(|rest| (marker, rest)))
    {
        return format!(
            "{indentation}{}{}",
            presentation::paint(marker, presentation::Tone::Success, false),
            color_inline_code(rest)
        );
    }
    if let Some(rest) = trimmed.strip_prefix("> ") {
        return format!(
            "{indentation}{}{}",
            presentation::paint("> ", presentation::Tone::Muted, false),
            color_inline_code(rest)
        );
    }
    color_inline_code(line)
}

fn color_inline_code(line: &str) -> String {
    if line.trim().is_empty() {
        return line.to_owned();
    }
    let mut result = String::with_capacity(line.len());
    let mut remaining = line;
    while let Some(start) = remaining.find('`') {
        result.push_str(&presentation::paint(
            &remaining[..start],
            presentation::Tone::Secondary,
            false,
        ));
        let code_start = start + 1;
        let Some(end) = remaining[code_start..].find('`') else {
            result.push_str(&presentation::paint(
                &remaining[start..],
                presentation::Tone::Secondary,
                false,
            ));
            break;
        };
        let code_end = code_start + end;
        result.push_str(&presentation::paint("`", presentation::Tone::Muted, false));
        result.push_str(&presentation::paint(
            &remaining[code_start..code_end],
            presentation::Tone::Template,
            false,
        ));
        result.push_str(&presentation::paint("`", presentation::Tone::Muted, false));
        remaining = &remaining[code_end + 1..];
    }
    result.push_str(&presentation::paint(
        remaining,
        presentation::Tone::Secondary,
        false,
    ));
    result
}

fn compact_output_paths(
    outputs: &[inifra_core::output_rules::ResolvedPackOutput],
    directories: &[PathBuf],
) -> Vec<String> {
    let files = compact_paths(outputs.iter().map(|output| output.output.as_path()), false);
    let directories = compact_paths(directories.iter().map(PathBuf::as_path), true);
    files.into_iter().chain(directories).collect()
}

#[derive(Default)]
struct OutputPathNode {
    terminal: bool,
    children: BTreeMap<String, OutputPathNode>,
}

fn compact_paths<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
    directories: bool,
) -> Option<String> {
    let mut root = OutputPathNode::default();
    for path in paths {
        let mut node = &mut root;
        let mut has_component = false;
        for component in path.components() {
            if let Component::Normal(name) = component {
                node = node
                    .children
                    .entry(name.to_string_lossy().into_owned())
                    .or_default();
                has_component = true;
            }
        }
        if has_component {
            node.terminal = true;
        }
    }

    let entries = root
        .children
        .iter()
        .map(|(name, node)| compact_path_entry(name, node, directories))
        .collect::<Vec<_>>();
    (!entries.is_empty()).then(|| entries.join(", "))
}

fn compact_path_entry(name: &str, node: &OutputPathNode, directories: bool) -> String {
    let mut path = name.to_owned();
    let mut current = node;
    while !current.terminal && current.children.len() == 1 {
        let Some((child_name, child)) = current.children.first_key_value() else {
            break;
        };
        path.push('/');
        path.push_str(child_name);
        current = child;
    }

    if current.children.is_empty() {
        if directories && current.terminal {
            path.push('/');
        }
        return path;
    }

    let children = current
        .children
        .iter()
        .map(|(child_name, child)| compact_path_entry(child_name, child, directories))
        .collect::<Vec<_>>()
        .join(", ");
    let nested = format!("{path}/{{{children}}}");
    if directories && current.terminal {
        format!("{path}/, {nested}")
    } else {
        nested
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

fn fit_list_description(id: &str, description: &str, width: Option<usize>) -> String {
    let Some(width) = width else {
        return description.to_owned();
    };
    let prefix_width = 2 + presentation::visible_width(id) + 2;
    presentation::truncate_to_width(description, width.saturating_sub(prefix_width))
}

fn fit_list_content(
    id: &str,
    description: &str,
    summary: &str,
    width: Option<usize>,
) -> (String, Option<String>) {
    let Some(width) = width else {
        return (
            description.to_owned(),
            (!summary.is_empty()).then(|| summary.to_owned()),
        );
    };

    let prefix_width = 2 + presentation::visible_width(id) + 2;
    let full_width = prefix_width
        + presentation::visible_width(description)
        + if summary.is_empty() {
            0
        } else {
            2 + presentation::visible_width(summary)
        };
    if full_width <= width {
        return (
            description.to_owned(),
            (!summary.is_empty()).then(|| summary.to_owned()),
        );
    }

    (
        presentation::truncate_to_width(description, width.saturating_sub(prefix_width)),
        None,
    )
}

#[cfg(test)]
mod presentation_tests {
    use super::fit_list_content;
    use inifra_tui::presentation::visible_width;

    #[test]
    fn list_omits_secondary_file_summary_when_it_would_wrap() {
        let id = "github-pull-request";
        let description = "GitHub pull request description template";
        let summary = ".github/pull_request_template.md";
        let (fitted_description, fitted_summary) =
            fit_list_content(id, description, summary, Some(55));

        assert!(fitted_summary.is_none());
        assert!(fitted_description.ends_with('…'));
        assert!(visible_width(&format!("  {id}  {fitted_description}")) <= 55);
    }

    #[test]
    fn list_keeps_file_summary_when_it_fits_on_the_same_line() {
        let id = "readme";
        let description = "Project overview";
        let summary = "README.md";
        let (fitted_description, fitted_summary) =
            fit_list_content(id, description, summary, Some(80));

        assert_eq!(fitted_description, description);
        assert_eq!(fitted_summary.as_deref(), Some(summary));
    }

    #[test]
    fn list_keeps_complete_content_without_a_terminal_width() {
        let id = "github-pull-request";
        let description = "GitHub pull request description template";
        let summary = ".github/pull_request_template.md";
        let (fitted_description, fitted_summary) = fit_list_content(id, description, summary, None);

        assert_eq!(fitted_description, description);
        assert_eq!(fitted_summary.as_deref(), Some(summary));
    }
}
