use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::cli::PresetCommand;
use crate::theme::{self, Tone};
use crate::{presets, templates};

pub(super) fn run(command: PresetCommand) -> Result<()> {
    match command {
        PresetCommand::List => list(),
        PresetCommand::Save {
            name,
            description,
            templates: template_ids,
            inputs,
            overwrite,
        } => save(&name, &description, &template_ids, &inputs, overwrite),
    }
}

fn list() -> Result<()> {
    let presets = presets::list()?;
    if presets.is_empty() {
        println!(
            "{}",
            theme::paint_stdout("No saved presets.", Tone::Muted, false)
        );
        return Ok(());
    }
    println!(
        "{}",
        theme::paint_stdout("Saved presets:", Tone::Accent, true)
    );
    for preset in presets {
        println!(
            "  {} {} [{}]",
            theme::paint_stdout(format!("{:<20}", preset.name), Tone::Accent, true),
            preset.description,
            theme::paint_stdout(preset.templates.join(", "), Tone::Success, false)
        );
    }
    Ok(())
}

fn save(
    name: &str,
    description: &str,
    template_ids: &[String],
    assignments: &[String],
    overwrite: bool,
) -> Result<()> {
    if template_ids.is_empty() {
        bail!("save at least one template with --template ID");
    }
    let catalog = templates::discover()?;
    let mut input_names = Vec::new();
    let mut seen_ids = HashSet::new();
    for template_id in template_ids {
        if !seen_ids.insert(template_id) {
            bail!("template {template_id:?} was selected more than once");
        }
        let Some(template) = catalog.iter().find(|template| template.id == *template_id) else {
            bail!("unknown template {template_id:?}");
        };
        for input in &template.inputs {
            if !input_names.contains(input) {
                input_names.push(input.clone());
            }
        }
    }
    let values = templates::parse_assignments(&input_names, name, assignments)?;
    let preset = presets::Preset::new(
        name.to_owned(),
        description.to_owned(),
        template_ids.to_vec(),
        values,
    );
    presets::save(&preset, overwrite)?;
    println!(
        "{} {}.",
        theme::paint_stdout("Saved preset", Tone::Success, true),
        theme::paint_stdout(format!("{:?}", preset.name), Tone::Accent, false)
    );
    Ok(())
}
