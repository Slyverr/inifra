use std::path::Path;

use anyhow::Result;

use crate::templates;
use crate::theme::{self, Tone};

pub(super) fn run(project_directory: Option<&Path>, filter: Option<&str>) -> Result<()> {
    let query = filter.map(templates::TemplateQuery::new);
    let mut templates = templates::discover_for(project_directory)?;
    if let Some(query) = query {
        templates.retain(|template| query.matches(template, template.category.as_deref()));
    }

    println!(
        "{}",
        theme::paint_stdout("Available templates:", Tone::Accent, true)
    );
    if templates.is_empty()
        && let Some(filter) = filter
    {
        println!(
            "{}",
            theme::paint_stdout(
                format!("No templates match {filter:?}."),
                Tone::Muted,
                false
            )
        );
        return Ok(());
    }

    for template in templates {
        let category = template
            .category
            .as_deref()
            .map(|category| {
                format!(
                    " {}",
                    theme::paint_stdout(format!("({category})"), Tone::Muted, false)
                )
            })
            .unwrap_or_default();
        let outputs = template
            .files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {} {} {}{} — {}",
            theme::paint_stdout(format!("{:<16}", template.id), Tone::Accent, true),
            template.description,
            theme::paint_stdout(format!("[{}]", template.origin.label()), Tone::Muted, false,),
            category,
            theme::paint_stdout(outputs, Tone::Path, false)
        );
    }
    Ok(())
}
