use std::path::Path;

use anyhow::{Context, Result};

use crate::{output, templates};

pub(super) fn run(
    template_id: &str,
    destination: &Path,
    dry_run: bool,
    overwrite: bool,
) -> Result<()> {
    let template = templates::find(template_id).ok_or_else(|| {
        let available = templates::all()
            .iter()
            .map(|template| template.id)
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::anyhow!("unknown template {template_id:?}; available templates: {available}")
    })?;

    let plan = output::plan(destination, template, overwrite)?;
    output::show_plan(&plan, dry_run);
    if dry_run {
        return Ok(());
    }

    if plan.all_skipped() {
        println!("No files written; existing files were left unchanged.");
        return Ok(());
    }

    let summary = output::write_plan(&plan)
        .with_context(|| format!("could not generate template {template_id:?}"))?;
    let written = summary.created + summary.replaced;
    let file_word = if written == 1 { "file" } else { "files" };
    println!(
        "Wrote {written} {file_word} ({} created, {} replaced, {} skipped).",
        summary.created, summary.replaced, summary.skipped
    );
    Ok(())
}
