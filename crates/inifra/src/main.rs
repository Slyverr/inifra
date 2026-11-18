use clap::{Parser, Subcommand};
use inifra_core::{apply, catalog, inputs, render, sources, workflow};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::PathBuf,
};

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
            for pack in &packs {
                println!("{}\t{}\t{}", pack.category, pack.id, pack.description);
            }
        }
        Command::Inspect { id } => {
            let pack = packs
                .iter()
                .find(|pack| pack.id == id)
                .ok_or_else(|| format!("unknown template {id:?}"))?;
            println!("{}: {}", pack.id, pack.description);
            for source in sources::discover(&pack.directory)? {
                println!("  {}", source.output.display());
            }
        }
        Command::Validate => {
            let definitions = inputs::load(&catalog_path.join("inputs.toml"))?;
            for pack in &packs {
                for name in &pack.inputs {
                    if !definitions.contains_key(name) {
                        return Err(format!(
                            "template {:?} references unknown input {name:?}",
                            pack.id
                        )
                        .into());
                    }
                }
                let files = sources::discover(&pack.directory)?;
                if files.is_empty() {
                    return Err(format!("template {:?} has no sources", pack.id).into());
                }
                let mut outputs = BTreeSet::new();
                for source in files {
                    if !outputs.insert(source.output.to_string_lossy().to_lowercase()) {
                        return Err(
                            format!("template {:?} has conflicting outputs", pack.id).into()
                        );
                    }
                    if source.render {
                        render::validate_template(&pack.directory.join(source.source))?;
                    }
                }
            }
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
                println!("{:?} {}", file.action, file.path.display());
            }
            if !dry_run {
                apply::apply(&target, &plan)?;
            }
        }
    }
    Ok(())
}
