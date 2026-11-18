use clap::{Parser, Subcommand};
use inifra_core::{apply, catalog, workflow};
use std::{collections::BTreeMap, error::Error, path::PathBuf};

#[derive(Parser)]
#[command(version, about = "Create project files from reusable templates")]
struct Cli {
    /// Path to a template catalog.
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List available template packs.
    List,
    /// Show details and outputs for a pack.
    Inspect { id: String },
    /// Validate catalog manifests and source paths.
    Validate,
    /// Generate files from selected template packs.
    Generate {
        #[arg(required = true)]
        ids: Vec<String>,
        #[arg(long, default_value = ".")]
        target: PathBuf,
        #[arg(long, value_name = "NAME=VALUE")]
        set: Vec<String>,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let temporary_catalog = if cli.catalog.is_none() {
        let dir = tempfile::tempdir()?;
        catalog::extract_bundled(dir.path())?;
        Some(dir)
    } else {
        None
    };
    let catalog_path = cli
        .catalog
        .as_deref()
        .or_else(|| temporary_catalog.as_ref().map(|dir| dir.path()))
        .ok_or("catalog unavailable")?;
    let catalog = catalog::Catalog::load(catalog_path)?;
    match cli.command {
        Command::List => {
            let mut category = "";
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
                    println!("{}", catalog.categories[category].name);
                }
                println!("  {:width$}  {}", pack.id, pack.description);
            }
        }
        Command::Inspect { id } => {
            let pack = catalog
                .packs
                .iter()
                .find(|pack| pack.id == id)
                .ok_or_else(|| format!("unknown template {id:?}"))?;
            println!("{}: {}", pack.id, pack.description);
            let output_paths = catalog.resolve_outputs(std::slice::from_ref(&id))?;
            if !output_paths.is_empty() {
                println!("Files:");
                for output in output_paths {
                    println!("  {}", output.output.display());
                    if let Some(alternative) = output.co_selected_output {
                        println!(
                            "    when paired ({})  {}",
                            alternative.group,
                            alternative.path.display()
                        );
                    }
                }
            }
            if !pack.directories.is_empty() {
                println!("Directories:");
                for directory in &pack.directories {
                    println!("  {}/", directory.display());
                }
            }
            let definitions = catalog.input_definitions_for(pack)?;
            if !definitions.is_empty() {
                println!("Inputs:");
                for (name, definition) in definitions {
                    let requirement = if definition.required {
                        "required"
                    } else {
                        "optional"
                    };
                    println!("  {name} ({requirement})  {}", definition.prompt);
                    if let Some(default) = &definition.default {
                        println!("    Default: {default}");
                    }
                }
            }
        }
        Command::Validate => {
            catalog.validate()?;
            println!("Validated {} templates", catalog.packs.len());
        }
        Command::Generate {
            ids,
            target,
            set,
            overwrite,
            dry_run,
        } => {
            let mut values = BTreeMap::new();
            for assignment in set {
                let (key, value) = assignment
                    .split_once('=')
                    .ok_or("--set expects NAME=VALUE")?;
                if values.insert(key.to_owned(), value.to_owned()).is_some() {
                    return Err(format!("duplicate input {key:?}").into());
                }
            }
            let plan = workflow::prepare(&catalog, &ids, &values, &target, overwrite)?;
            for directory in &plan.directories {
                let action = match directory.action {
                    inifra_core::plan::DirectoryAction::Create => "create",
                    inifra_core::plan::DirectoryAction::Existing => "skip (already exists)",
                };
                println!("{action}: {}/", directory.path.display());
            }
            for file in &plan.files {
                let action = match file.action {
                    inifra_core::plan::Action::Create => "create",
                    inifra_core::plan::Action::Replace => "replace",
                    inifra_core::plan::Action::Skip => "skip (already exists)",
                };
                println!("{action}: {}", file.path.display());
            }
            if !dry_run {
                apply::apply_plan(&target, &plan)?;
            }
        }
    }
    Ok(())
}
