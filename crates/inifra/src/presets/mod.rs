use clap::Subcommand;
use inifra_core::{catalog::Catalog, presets, workflow};
use inifra_tui::presentation;
use std::{error::Error, path::Path};

#[derive(Debug, Subcommand)]
#[command(styles = crate::cli_style::styles())]
pub(crate) enum PresetCommand {
    /// List saved presets.
    List,
    /// Save a template selection and optional input values.
    Save {
        /// Preset ID.
        id: String,
        /// Short description shown by preset list.
        #[arg(long, required = true)]
        description: String,
        /// Template ID to include (repeatable).
        #[arg(
            long = "template",
            value_name = "ID",
            required = true,
            add = crate::completions::template_completer()
        )]
        templates: Vec<String>,
        /// Set an input value as NAME=VALUE (repeatable).
        #[arg(
            long = "set",
            value_name = "NAME=VALUE",
            add = crate::completions::input_completer()
        )]
        values: Vec<String>,
        /// Replace an existing preset with this ID.
        #[arg(long)]
        overwrite: bool,
    },
    /// Show the templates and saved inputs in a preset.
    Inspect {
        /// Preset ID.
        #[arg(add = crate::completions::preset_completer())]
        id: String,
    },
    /// Apply a preset through the interactive selection and review flow.
    Apply {
        /// Preset ID.
        #[arg(add = crate::completions::preset_completer())]
        id: String,
        /// Destination directory.
        #[arg(long, default_value = ".")]
        target: std::path::PathBuf,
        /// Override a saved input as NAME=VALUE (repeatable).
        #[arg(
            long = "set",
            value_name = "NAME=VALUE",
            add = crate::completions::input_completer()
        )]
        values: Vec<String>,
        /// Replace existing files after review.
        #[arg(long)]
        overwrite: bool,
    },
    /// Rename a saved preset.
    Rename {
        /// Current preset ID.
        #[arg(add = crate::completions::preset_completer())]
        old_id: String,
        /// New preset ID.
        new_id: String,
    },
    /// Remove a saved preset.
    Remove {
        /// Preset ID.
        #[arg(add = crate::completions::preset_completer())]
        id: String,
    },
}

pub(crate) fn run(
    catalog: &Catalog,
    config_dir: Option<&Path>,
    command: PresetCommand,
) -> Result<(), Box<dyn Error>> {
    let config_dir = config_dir.ok_or("could not determine the user config directory")?;
    let root = presets::store_path(config_dir);
    match command {
        PresetCommand::List => list(&root, catalog),
        PresetCommand::Save {
            id,
            description,
            templates,
            values,
            overwrite,
        } => save(
            &root,
            catalog,
            &id,
            &description,
            &templates,
            &values,
            overwrite,
        ),
        PresetCommand::Inspect { id } => inspect(&root, catalog, &id),
        PresetCommand::Apply {
            id,
            target,
            values,
            overwrite,
        } => apply(&root, catalog, &id, target, &values, overwrite),
        PresetCommand::Rename { old_id, new_id } => {
            presets::rename(&root, &old_id, &new_id, catalog)?;
            println!(
                "{} {} to {}.",
                presentation::paint("Renamed preset", presentation::Tone::Success, false),
                presentation::paint_emphasis(&old_id, presentation::Tone::Muted),
                presentation::paint_emphasis(&new_id, presentation::Tone::Accent)
            );
            Ok(())
        }
        PresetCommand::Remove { id } => {
            presets::remove(&root, &id)?;
            println!(
                "{} {}.",
                presentation::paint("Removed preset", presentation::Tone::Success, false),
                presentation::paint_emphasis(&id, presentation::Tone::Accent)
            );
            Ok(())
        }
    }
}

fn list(root: &Path, catalog: &Catalog) -> Result<(), Box<dyn Error>> {
    let saved = presets::load_all(root, catalog)?;
    if saved.is_empty() {
        println!(
            "{}",
            presentation::paint("No saved presets.", presentation::Tone::Muted, false)
        );
        return Ok(());
    }

    let width = saved
        .iter()
        .map(|preset| preset.id.len())
        .max()
        .unwrap_or(0);
    for preset in saved {
        println!(
            "{}  {}  {}",
            presentation::paint_emphasis(
                format!("{:width$}", presentation::terminal_text(&preset.id)),
                presentation::Tone::Accent
            ),
            presentation::terminal_text(&preset.description),
            presentation::paint(
                format!("[{}]", preset.templates.join(", ")),
                presentation::Tone::Muted,
                false
            )
        );
    }

    Ok(())
}

fn save(
    root: &Path,
    catalog: &Catalog,
    id: &str,
    description: &str,
    template_ids: &[String],
    assignments: &[String],
    overwrite: bool,
) -> Result<(), Box<dyn Error>> {
    crate::commands::init::validate_picker_selection(catalog, template_ids)?;
    let preset = presets::Preset {
        id: id.to_owned(),
        description: description.to_owned(),
        templates: template_ids.to_vec(),
        values: crate::inputs::parse_assignments(assignments)?,
    };
    presets::save(root, &preset, catalog, overwrite)?;
    println!(
        "{} {}",
        presentation::paint("Saved preset", presentation::Tone::Success, false),
        presentation::paint_emphasis(&preset.id, presentation::Tone::Accent)
    );
    Ok(())
}

fn inspect(root: &Path, catalog: &Catalog, id: &str) -> Result<(), Box<dyn Error>> {
    let preset = presets::load_all(root, catalog)?
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| format!("unknown preset {id:?}"))?;
    println!(
        "{}",
        presentation::paint_emphasis(&preset.id, presentation::Tone::Accent)
    );
    println!("{}", presentation::terminal_text(&preset.description));
    println!();
    println!(
        "{}",
        presentation::paint_emphasis("Templates", presentation::Tone::Accent)
    );
    for template in &preset.templates {
        println!(
            "  {}",
            presentation::paint_emphasis(template, presentation::Tone::Accent)
        );
    }

    if !preset.values.is_empty() {
        println!();
        println!(
            "{}",
            presentation::paint_emphasis("Saved inputs", presentation::Tone::Accent)
        );
        for (name, value) in &preset.values {
            println!(
                "  {} = {}",
                presentation::paint_emphasis(
                    presentation::terminal_text(name),
                    presentation::Tone::Warning
                ),
                presentation::terminal_text(value)
            );
        }
    }

    Ok(())
}

fn apply(
    root: &Path,
    catalog: &Catalog,
    id: &str,
    target: std::path::PathBuf,
    assignments: &[String],
    overwrite: bool,
) -> Result<(), Box<dyn Error>> {
    let preset = presets::load_all(root, catalog)?
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| format!("unknown preset {id:?}"))?;
    let mut values = preset.values;
    values.extend(crate::inputs::parse_assignments(assignments)?);
    let definitions = workflow::input_definitions_for_selection(catalog, &preset.templates)?;
    if let Some(name) = values.keys().find(|name| !definitions.contains_key(*name)) {
        return Err(format!("input {name:?} is not used by preset {id:?}").into());
    }
    crate::commands::init::run_with_selection(catalog, target, overwrite, &preset.templates, values)
}
