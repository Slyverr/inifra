use clap::Args;
use inifra_core::catalog::Catalog;
use std::error::Error;

use crate::{output::catalog as catalog_output, presentation};

#[derive(Debug, Args)]
pub(crate) struct ListArgs {
    /// Show every file and directory output for each pack.
    #[arg(long)]
    pub(crate) files: bool,
}

pub(crate) fn run(catalog: &Catalog, args: ListArgs) -> Result<(), Box<dyn Error>> {
    let mut category = "";
    let terminal_width = presentation::terminal_width();
    let width = catalog
        .packs
        .iter()
        .map(|pack| pack.id.len())
        .max()
        .unwrap_or(0);

    for pack in &catalog.packs {
        if category != pack.category {
            if !category.is_empty() {
                println!();
            }
            category = &pack.category;
            let name = presentation::terminal_text(&catalog.categories[category].name);
            let name = terminal_width.map_or(name.clone(), |width| {
                presentation::truncate_to_width(&name, width)
            });
            println!(
                "{}",
                presentation::paint_emphasis(name, presentation::Tone::Muted)
            );
        }

        let selected = std::slice::from_ref(&pack.id);
        let outputs = catalog.resolve_outputs(selected)?;
        let id = format!("{:width$}", presentation::terminal_text(&pack.id));
        let description = presentation::terminal_text(&pack.description);

        if args.files {
            let description =
                catalog_output::fit_list_description(&id, &description, terminal_width);
            println!(
                "  {}  {description}",
                presentation::paint_emphasis(&id, presentation::Tone::Accent)
            );
            for paths in catalog_output::compact_output_paths(&outputs, &pack.directories) {
                println!(
                    "    {}",
                    presentation::paint(
                        presentation::terminal_text(&paths),
                        presentation::Tone::Muted,
                        false
                    )
                );
            }
        } else {
            let summary = catalog_output::compact_output_summary(&outputs, &pack.directories);
            let (description, summary) =
                catalog_output::fit_list_content(&id, &description, &summary, terminal_width);
            let output_summary = summary.map_or_else(String::new, |summary| {
                format!(
                    "  {}",
                    presentation::paint(
                        presentation::terminal_text(&summary),
                        presentation::Tone::Muted,
                        false
                    )
                )
            });
            println!(
                "  {}  {description}{output_summary}",
                presentation::paint_emphasis(&id, presentation::Tone::Accent)
            );
        }
    }

    Ok(())
}
