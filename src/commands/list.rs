use anyhow::Result;

use crate::templates::{self, TemplateOrigin};

pub(super) fn run() -> Result<()> {
    println!("Available templates:");
    for template in templates::discover()? {
        let origin = match template.origin {
            TemplateOrigin::Builtin => "built-in",
            TemplateOrigin::Local => "local",
        };
        let category = template
            .category
            .as_deref()
            .map(|category| format!(" ({category})"))
            .unwrap_or_default();
        let outputs = template
            .files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {:<16} {} [{}]{} — {}",
            template.id, template.description, origin, category, outputs
        );
    }
    Ok(())
}
