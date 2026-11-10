use anyhow::Result;

use crate::templates;

pub(super) fn run() -> Result<()> {
    println!("Available templates:");
    for template in templates::all() {
        let outputs = template
            .outputs
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {:<16} {} — {}",
            template.id, template.description, outputs
        );
    }
    Ok(())
}
