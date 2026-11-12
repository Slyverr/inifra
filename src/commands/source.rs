use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::SourceCommand;
use crate::templates::{self, SourceRegistry};
use crate::theme::{self, Tone};

pub(super) fn run(command: SourceCommand) -> Result<()> {
    match command {
        SourceCommand::List { path } => list(&path),
        SourceCommand::Add {
            name,
            url,
            revision,
            directory,
            path,
        } => add(&path, &name, &url, &revision, &directory),
        SourceCommand::Sync { path } => sync(&path),
        SourceCommand::Update {
            name,
            revision,
            path,
        } => update(&path, &name, &revision),
        SourceCommand::Remove { name, path } => remove(&path, &name),
    }
}

fn list(project: &Path) -> Result<()> {
    let registry = templates::read_project_registry(project)?;
    if registry.sources.is_empty() {
        println!("No project template sources are configured.");
        return Ok(());
    }

    println!("Project template sources:");
    for (name, source) in registry.sources {
        let synced = if templates::source_is_cached(&name, &source)? {
            "synced"
        } else {
            "not synced"
        };
        println!(
            "  {} {} {} [{}]",
            theme::paint_stdout(&name, Tone::Accent, true),
            source.url,
            theme::paint_stdout(&source.revision, Tone::Muted, false),
            synced
        );
    }
    Ok(())
}

fn add(project: &Path, name: &str, url: &str, revision: &str, directory: &str) -> Result<()> {
    ensure_project_directory(project)?;
    templates::validate_id(name)?;
    let mut registry = templates::read_project_registry(project)?;
    if registry.sources.contains_key(name) {
        bail!("project template source {name:?} already exists; use `inifra source update`");
    }

    let source = templates::fetch_source(name, url, revision, directory)?;
    registry.sources.insert(name.to_owned(), source.clone());
    templates::write_project_registry(project, &registry)?;
    println!(
        "Added source {} at {}",
        theme::paint_stdout(name, Tone::Success, true),
        theme::paint_stdout(source.revision, Tone::Muted, false)
    );
    Ok(())
}

fn sync(project: &Path) -> Result<()> {
    ensure_project_directory(project)?;
    let registry = templates::read_project_registry(project)?;
    templates::sync_project_sources(project)?;
    println!(
        "Synced {} project template source(s).",
        registry.sources.len()
    );
    Ok(())
}

fn update(project: &Path, name: &str, revision: &str) -> Result<()> {
    ensure_project_directory(project)?;
    let mut registry = templates::read_project_registry(project)?;
    let current = registry
        .sources
        .get(name)
        .with_context(|| format!("unknown project template source {name:?}"))?;
    let source = templates::fetch_source(name, &current.url, revision, &current.directory)?;
    registry.sources.insert(name.to_owned(), source.clone());
    templates::write_project_registry(project, &registry)?;
    println!(
        "Updated source {} to {}",
        theme::paint_stdout(name, Tone::Success, true),
        theme::paint_stdout(source.revision, Tone::Muted, false)
    );
    Ok(())
}

fn remove(project: &Path, name: &str) -> Result<()> {
    ensure_project_directory(project)?;
    let mut registry: SourceRegistry = templates::read_project_registry(project)?;
    if registry.sources.remove(name).is_none() {
        bail!("unknown project template source {name:?}");
    }
    templates::write_project_registry(project, &registry)?;
    println!("Removed project template source {name:?}.");
    Ok(())
}

fn ensure_project_directory(project: &Path) -> Result<()> {
    let metadata = std::fs::metadata(project)
        .with_context(|| format!("could not inspect project directory {}", project.display()))?;
    if !metadata.is_dir() {
        bail!("project path must be a directory: {}", project.display());
    }
    Ok(())
}
