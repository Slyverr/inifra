use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::templates::{self, Template};
use crate::theme::{self, Tone};

pub(super) fn run(template_id: Option<&str>, project_directory: Option<&Path>) -> Result<()> {
    let mut catalog = templates::discover_for(project_directory)?;
    if let Some(template_id) = template_id {
        catalog.retain(|template| template.id == template_id);
        if catalog.is_empty() {
            bail!("unknown template {template_id:?}");
        }
    }

    for mut template in catalog {
        validate_template(&mut template, project_directory)
            .with_context(|| format!("could not validate template {:?}", template.id))?;
        println!(
            "{} {}",
            theme::paint_stdout("valid", Tone::Success, true),
            theme::paint_stdout(template.id, Tone::Accent, false)
        );
    }
    Ok(())
}

fn validate_template(template: &mut Template, project_directory: Option<&Path>) -> Result<()> {
    let values = templates::validation_values(&template.id, &template.inputs, project_directory)?;
    templates::render_text_files(template, &values)?;
    Ok(())
}
