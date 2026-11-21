use clap::Args;
use inifra_core::catalog::Catalog;
use std::error::Error;

use crate::{completions, presentation};

#[derive(Debug, Args)]
pub(crate) struct InspectArgs {
    #[arg(add = completions::template_completer())]
    pub(crate) id: String,
}

pub(crate) fn run(catalog: &Catalog, args: InspectArgs) -> Result<(), Box<dyn Error>> {
    let pack = catalog
        .packs
        .iter()
        .find(|pack| pack.id == args.id)
        .ok_or_else(|| format!("unknown template {:?}", args.id))?;

    println!(
        "{}",
        presentation::paint_emphasis(
            presentation::terminal_text(&pack.id),
            presentation::Tone::Accent
        )
    );
    println!("{}", presentation::terminal_text(&pack.description));

    let output_paths = catalog.resolve_outputs(std::slice::from_ref(&args.id))?;
    if !output_paths.is_empty() {
        println!();
        println!(
            "{}",
            presentation::paint_emphasis("Files:", presentation::Tone::Accent)
        );
        for output in output_paths {
            println!(
                "  {}",
                presentation::paint(
                    presentation::terminal_text(&output.output.display().to_string()),
                    presentation::Tone::Path,
                    false
                )
            );
            if let Some(alternative) = output.co_selected_output {
                println!(
                    "    {}  {}",
                    presentation::paint(
                        format!(
                            "when paired ({})",
                            presentation::terminal_text(&alternative.group)
                        ),
                        presentation::Tone::Muted,
                        false
                    ),
                    presentation::paint(
                        presentation::terminal_text(&alternative.path.display().to_string()),
                        presentation::Tone::Path,
                        false
                    )
                );
            }
        }
    }

    if !pack.directories.is_empty() {
        println!();
        println!(
            "{}",
            presentation::paint_emphasis("Directories:", presentation::Tone::Accent)
        );
        for directory in &pack.directories {
            println!(
                "  {}",
                presentation::paint(
                    format!(
                        "{}/",
                        presentation::terminal_text(&directory.display().to_string())
                    ),
                    presentation::Tone::Path,
                    false
                )
            );
        }
    }

    let definitions = catalog.input_definitions_for(pack)?;
    if !definitions.is_empty() {
        println!();
        println!(
            "{}",
            presentation::paint_emphasis("Inputs:", presentation::Tone::Accent)
        );
        for (name, definition) in definitions {
            let requirement = if definition.required {
                "required"
            } else {
                "optional"
            };
            println!(
                "  {} {}  {}",
                presentation::paint_emphasis(
                    presentation::terminal_text(&name),
                    presentation::Tone::Warning
                ),
                presentation::paint(format!("({requirement})"), presentation::Tone::Muted, false),
                presentation::terminal_text(&definition.prompt)
            );
            if let Some(default) = &definition.default {
                println!(
                    "    {} {}",
                    presentation::paint("Default:", presentation::Tone::Muted, false),
                    presentation::paint_emphasis(
                        presentation::terminal_text(default),
                        presentation::Tone::Warning
                    )
                );
            }
        }
    }

    Ok(())
}
