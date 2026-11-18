use clap::{Parser, Subcommand};
use inifra_core::{apply, catalog, inputs, sources, workflow};
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
    let packs = catalog::discover(catalog_path)?;
    match cli.command {
        Command::List => {
            let mut category = "";
            let width = packs.iter().map(|pack| pack.id.len()).max().unwrap_or(0);
            for pack in &packs {
                if category != pack.category {
                    if !category.is_empty() {
                        println!();
                    }
                    category = &pack.category;
                    let label = category
                        .split('-')
                        .map(|word| {
                            let mut chars = word.chars();
                            chars
                                .next()
                                .map(|first| {
                                    first.to_uppercase().collect::<String>() + chars.as_str()
                                })
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    println!("{label}");
                }
                println!("  {:width$}  {}", pack.id, pack.description);
            }
        }
        Command::Inspect { id } => {
            let pack = packs
                .iter()
                .find(|pack| pack.id == id)
                .ok_or_else(|| format!("unknown template {id:?}"))?;
            println!("{}: {}", pack.id, pack.description);
            println!("Files:");
            for source in sources::discover(&pack.directory)? {
                println!("  {}", source.output.display());
            }
            if !pack.inputs.is_empty() {
                let definitions = inputs::load(&catalog_path.join("inputs.toml"))?;
                println!("Inputs:");
                for name in &pack.inputs {
                    let definition = definitions.get(name).ok_or_else(|| {
                        format!("template {:?} references unknown input {name:?}", pack.id)
                    })?;
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
            catalog::validate(catalog_path, &packs)?;
            println!("Validated {} templates", packs.len());
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
            let definitions = inputs::load(&catalog_path.join("inputs.toml"))?;
            let plan = workflow::prepare(&packs, &definitions, &ids, &values, &target, overwrite)?;
            for file in &plan {
                let action = match file.action {
                    inifra_core::plan::Action::Create => "create",
                    inifra_core::plan::Action::Replace => "replace",
                    inifra_core::plan::Action::Skip => "skip (already exists)",
                };
                println!("{action}: {}", file.path.display());
            }
            if !dry_run {
                apply::apply(&target, &plan)?;
            }
        }
    }
    Ok(())
}
