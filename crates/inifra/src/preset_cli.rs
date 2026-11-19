use clap::Subcommand;
use inifra_core::{catalog::Catalog, presets, workflow};
use inifra_tui::presentation;
use std::{error::Error, path::Path};

#[derive(Debug, Subcommand)]
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
        #[arg(long = "template", value_name = "ID", required = true)]
        templates: Vec<String>,
        /// Set an input value as NAME=VALUE (repeatable).
        #[arg(long = "set", value_name = "NAME=VALUE")]
        values: Vec<String>,
        /// Replace an existing preset with this ID.
        #[arg(long)]
        overwrite: bool,
    },
    /// Show the templates and saved inputs in a preset.
    Inspect {
        /// Preset ID.
        id: String,
    },
    /// Apply a preset through the interactive selection and review flow.
    Apply {
        /// Preset ID.
        id: String,
        /// Destination directory.
        #[arg(long, default_value = ".")]
        target: std::path::PathBuf,
        /// Override a saved input as NAME=VALUE (repeatable).
        #[arg(long = "set", value_name = "NAME=VALUE")]
        values: Vec<String>,
        /// Replace existing files after review.
        #[arg(long)]
        overwrite: bool,
    },
    /// Rename a saved preset.
    Rename {
        /// Current preset ID.
        old_id: String,
        /// New preset ID.
        new_id: String,
    },
    /// Remove a saved preset.
    Remove {
        /// Preset ID.
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
                "Renamed preset {} to {}.",
                presentation::paint_emphasis(&old_id, presentation::Tone::Muted),
                presentation::paint_emphasis(&new_id, presentation::Tone::Accent)
            );
            Ok(())
        }
        PresetCommand::Remove { id } => {
            presets::remove(&root, &id)?;
            println!(
                "Removed preset {}.",
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
    crate::init::validate_picker_selection(catalog, template_ids)?;
    let preset = presets::Preset {
        id: id.to_owned(),
        description: description.to_owned(),
        templates: template_ids.to_vec(),
        values: crate::parse_assignments(assignments)?,
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
    println!("Templates");
    for template in &preset.templates {
        println!(
            "  {}",
            presentation::paint(template, presentation::Tone::Template, false)
        );
    }
    if !preset.values.is_empty() {
        println!();
        println!("Saved inputs");
        for (name, value) in &preset.values {
            println!(
                "  {} = {}",
                presentation::terminal_text(name),
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
    values.extend(crate::parse_assignments(assignments)?);
    let definitions = workflow::input_definitions_for_selection(catalog, &preset.templates)?;
    if let Some(name) = values.keys().find(|name| !definitions.contains_key(*name)) {
        return Err(format!("input {name:?} is not used by preset {id:?}").into());
    }
    crate::init::run_with_selection(catalog, target, overwrite, &preset.templates, values)
}
