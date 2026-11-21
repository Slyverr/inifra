use clap::Args;
use inifra_core::{apply, catalog::Catalog, workflow};
use std::{error::Error, path::PathBuf};

use crate::{completions, inputs, output::generation};

#[derive(Debug, Args)]
pub(crate) struct GenerateArgs {
    #[arg(required = true, value_name = "ID", add = completions::template_completer())]
    pub(crate) ids: Vec<String>,
    #[arg(long, default_value = ".")]
    pub(crate) target: PathBuf,
    #[arg(
        long,
        value_name = "NAME=VALUE",
        add = completions::input_completer()
    )]
    pub(crate) set: Vec<String>,
    #[arg(long)]
    pub(crate) overwrite: bool,
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Show new content and replacement diffs in a dry run.
    #[arg(long, requires = "dry_run")]
    pub(crate) show_content: bool,
}

pub(crate) fn run(catalog: &Catalog, args: GenerateArgs) -> Result<(), Box<dyn Error>> {
    let values = inputs::parse_assignments(&args.set)?;
    let plan = workflow::prepare(catalog, &args.ids, &values, &args.target, args.overwrite)?;

    if args.dry_run {
        generation::print_plan(&plan, &args.target, args.show_content);
        println!(
            "{}",
            crate::presentation::paint(
                "No files were changed.",
                crate::presentation::Tone::Muted,
                false
            )
        );
    } else {
        apply::apply_plan(&args.target, &plan)?;
        generation::print_applied(&plan, &args.target);
    }

    Ok(())
}
