use std::path::Path;

use anyhow::Result;

use crate::templates;
use crate::theme::{self, Tone};

pub(super) fn run(project_directory: Option<&Path>) -> Result<()> {
    println!(
        "{}",
        theme::paint_stdout("Available templates:", Tone::Accent, true)
    );
    for template in templates::discover_for(project_directory)? {
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
