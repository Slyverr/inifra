use anyhow::Result;

use crate::templates;

pub(super) fn run() -> Result<()> {
    println!("Available templates:");
    for template in templates::discover()? {
        let outputs = template
            .files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {:<16} {} — {}",
            template.id, template.description, outputs
        );
    }
    Ok(())
}
