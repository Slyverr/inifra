use clap::Args;
use inifra_core::catalog::Catalog;
use std::error::Error;

use crate::presentation;

#[derive(Debug, Args)]
pub(crate) struct ValidateArgs {}

pub(crate) fn run(catalog: &Catalog, _args: ValidateArgs) -> Result<(), Box<dyn Error>> {
    catalog.validate()?;
    for pack in &catalog.packs {
        println!(
            "{}  {}",
            presentation::paint("valid", presentation::Tone::Success, false),
            presentation::paint_emphasis(&pack.id, presentation::Tone::Accent)
        );
    }
    println!(
        "{} {} template(s).",
        presentation::paint("Validated", presentation::Tone::Success, false),
        presentation::paint_emphasis(catalog.packs.len().to_string(), presentation::Tone::Accent)
    );
    Ok(())
}
