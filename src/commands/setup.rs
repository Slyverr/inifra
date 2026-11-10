use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{output, presets, templates};

pub(super) fn run(
    template_ids: &[String],
    preset_name: Option<&str>,
    destination: &Path,
    dry_run: bool,
    overwrite: bool,
    assignments: &[String],
) -> Result<()> {
    if preset_name.is_some() && !template_ids.is_empty() {
        bail!("choose either --preset or --template options, not both");
    }
    let preset = preset_name.map(presets::load).transpose()?;
    let (selected_ids, preset_values) = match preset {
        Some(preset) => (preset.templates, preset.values),
        None => (template_ids.to_vec(), Default::default()),
    };
    if selected_ids.is_empty() {
        bail!("select at least one template with --template ID");
    }

    let catalog = templates::discover()?;
    let mut selected = Vec::with_capacity(selected_ids.len());
    let mut seen_ids = HashSet::with_capacity(selected_ids.len());
    let mut input_names = Vec::new();
    let mut seen_inputs = HashSet::new();
    for template_id in &selected_ids {
        if !seen_ids.insert(template_id) {
            bail!("template {template_id:?} was selected more than once");
        }
        let Some(template) = catalog.iter().find(|template| template.id == *template_id) else {
            let available = catalog
                .iter()
                .map(|template| template.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("unknown template {template_id:?}; available templates: {available}");
        };
        for input in &template.inputs {
            if seen_inputs.insert(input.clone()) {
                input_names.push(input.clone());
            }
        }
        selected.push(template.clone());
    }

    let selection = selected_ids.join(", ");
    let values = templates::resolve_values(
        &selection,
        &input_names,
        assignments,
        &preset_values,
        destination,
    )?;
    let mut files = Vec::new();
    for mut template in selected {
        templates::render_text_files(&mut template, &values)?;
        files.extend(template.files);
    }

    let plan = output::plan(destination, &selection, &files, overwrite)?;
    output::show_plan(&plan, dry_run);
    if dry_run {
        return Ok(());
    }
    if plan.all_skipped() {
        println!("No files written; existing files were left unchanged.");
        return Ok(());
    }

    let summary = output::write_plan(&plan)
        .with_context(|| format!("could not generate templates {selection:?}"))?;
    let written = summary.created + summary.replaced;
    let file_word = if written == 1 { "file" } else { "files" };
    println!(
        "Wrote {written} {file_word} ({} created, {} replaced, {} skipped).",
        summary.created, summary.replaced, summary.skipped
    );
    Ok(())
}
