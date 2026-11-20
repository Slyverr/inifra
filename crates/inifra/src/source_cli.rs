//! Explicit management of project-pinned Git template sources.

use clap::Subcommand;
use inifra_core::source_registry::{
    GitSource, SourceRegistry, validate_git_reference, validate_git_url, validate_source_name,
};
use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Subcommand)]
#[command(styles = crate::cli_style::styles())]
pub(crate) enum SourceCommand {
    /// Register a Git catalog and pin the selected ref at its current revision.
    Add {
        #[arg(value_name = "NAME")]
        name: String,
        #[arg(value_name = "GIT_URL")]
        url: String,
        #[arg(long = "ref", default_value = "HEAD", value_name = "REF")]
        reference: String,
    },
    /// List sources pinned by this project.
    List,
    /// Populate the cache for a pinned source without changing its revision.
    Sync {
        #[arg(value_name = "NAME", add = crate::completions::source_name_completer())]
        name: Option<String>,
    },
    /// Advance one or all source pins to their tracked refs.
    Update {
        #[arg(value_name = "NAME", add = crate::completions::source_name_completer())]
        name: Option<String>,
    },
    /// Remove a source from this project without deleting its shared cache.
    Remove {
        #[arg(value_name = "NAME", add = crate::completions::source_name_completer())]
        name: String,
    },
}

pub(crate) fn run(command: SourceCommand, project_dir: &Path) -> Result<(), Box<dyn Error>> {
    match command {
        SourceCommand::Add {
            name,
            url,
            reference,
        } => add(project_dir, &name, &url, &reference),
        SourceCommand::List => list(project_dir),
        SourceCommand::Sync { name } => sync(project_dir, name.as_deref()),
        SourceCommand::Update { name } => update(project_dir, name.as_deref()),
        SourceCommand::Remove { name } => remove(project_dir, &name),
    }
}

fn add(project_dir: &Path, name: &str, url: &str, reference: &str) -> Result<(), Box<dyn Error>> {
    validate_source_name(name)?;
    validate_git_url(url)?;
    validate_git_reference(reference)?;
    let (_, mut registry) = load_registry(project_dir)?;
    if registry.sources.contains_key(name) {
        return Err(format!("source {name:?} already exists").into());
    }

    let cache = source_cache_dir(name)?;
    let revision = crate::source_cache::fetch_ref(url, reference, &cache)?;
    registry.insert(
        name,
        GitSource {
            url: url.to_owned(),
            reference: reference.to_owned(),
            revision: revision.clone(),
        },
    )?;
    save_registry(project_dir, &registry)?;
    println!(
        "{} {}  {} @ {}",
        inifra_tui::presentation::paint(
            "Added source",
            inifra_tui::presentation::Tone::Success,
            false
        ),
        inifra_tui::presentation::paint_emphasis(name, inifra_tui::presentation::Tone::Accent),
        inifra_tui::presentation::terminal_text(reference),
        inifra_tui::presentation::terminal_text(&revision)
    );
    Ok(())
}

fn list(project_dir: &Path) -> Result<(), Box<dyn Error>> {
    let (_, registry) = load_registry(project_dir)?;
    if registry.sources.is_empty() {
        println!(
            "{}",
            inifra_tui::presentation::paint(
                "No project sources are configured.",
                inifra_tui::presentation::Tone::Muted,
                false
            )
        );
        return Ok(());
    }
    for (name, source) in registry.sources {
        println!(
            "{}  {} @ {}",
            inifra_tui::presentation::paint_emphasis(&name, inifra_tui::presentation::Tone::Accent),
            inifra_tui::presentation::terminal_text(&source.reference),
            inifra_tui::presentation::terminal_text(&source.revision)
        );
        println!(
            "  {}",
            inifra_tui::presentation::paint(
                inifra_tui::presentation::terminal_text(&source.url),
                inifra_tui::presentation::Tone::Muted,
                false
            )
        );
    }
    Ok(())
}

fn sync(project_dir: &Path, selected: Option<&str>) -> Result<(), Box<dyn Error>> {
    let (_, registry) = load_registry(project_dir)?;
    let names = selected_names(&registry, selected)?;
    if names.is_empty() {
        println!(
            "{}",
            inifra_tui::presentation::paint(
                "No project sources are configured.",
                inifra_tui::presentation::Tone::Muted,
                false
            )
        );
        return Ok(());
    }

    for name in names {
        let source = &registry.sources[&name];
        let cache = source_cache_dir(&name)?;
        let existed = crate::source_cache::cached_revision(&cache, &source.revision)?.is_some();
        crate::source_cache::fetch_revision(
            &source.url,
            &source.reference,
            &source.revision,
            &cache,
        )?;
        let action = if existed { "ready" } else { "cached" };
        println!(
            "{}  {} @ {}",
            inifra_tui::presentation::paint(action, inifra_tui::presentation::Tone::Success, false),
            inifra_tui::presentation::paint_emphasis(&name, inifra_tui::presentation::Tone::Accent),
            inifra_tui::presentation::terminal_text(&source.revision)
        );
    }
    Ok(())
}

fn update(project_dir: &Path, selected: Option<&str>) -> Result<(), Box<dyn Error>> {
    let (_, mut registry) = load_registry(project_dir)?;
    let names = selected_names(&registry, selected)?;
    if names.is_empty() {
        println!(
            "{}",
            inifra_tui::presentation::paint(
                "No project sources are configured.",
                inifra_tui::presentation::Tone::Muted,
                false
            )
        );
        return Ok(());
    }

    let mut updated = Vec::new();
    for name in names {
        let source = &registry.sources[&name];
        let revision = crate::source_cache::fetch_ref(
            &source.url,
            &source.reference,
            &source_cache_dir(&name)?,
        )?;
        if revision != source.revision {
            updated.push((name, revision));
        }
    }

    if updated.is_empty() {
        println!(
            "{}",
            inifra_tui::presentation::paint(
                "All selected source pins are up to date.",
                inifra_tui::presentation::Tone::Success,
                false
            )
        );
        return Ok(());
    }
    for (name, revision) in &updated {
        let mut source = registry.sources[name].clone();
        source.revision.clone_from(revision);
        registry.replace(name, source)?;
    }
    save_registry(project_dir, &registry)?;
    for (name, revision) in updated {
        println!(
            "{}  {} @ {}",
            inifra_tui::presentation::paint(
                "updated",
                inifra_tui::presentation::Tone::Success,
                false
            ),
            inifra_tui::presentation::paint_emphasis(&name, inifra_tui::presentation::Tone::Accent),
            inifra_tui::presentation::terminal_text(&revision)
        );
    }
    Ok(())
}

fn remove(project_dir: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    let (_, mut registry) = load_registry(project_dir)?;
    registry.remove(name)?;
    save_registry(project_dir, &registry)?;
    println!(
        "{} {}  {}",
        inifra_tui::presentation::paint(
            "Removed source",
            inifra_tui::presentation::Tone::Success,
            false
        ),
        inifra_tui::presentation::paint_emphasis(name, inifra_tui::presentation::Tone::Accent),
        inifra_tui::presentation::paint(
            "cached snapshot retained",
            inifra_tui::presentation::Tone::Muted,
            false
        )
    );
    Ok(())
}

fn selected_names(
    registry: &SourceRegistry,
    selected: Option<&str>,
) -> Result<Vec<String>, Box<dyn Error>> {
    match selected {
        Some(name) if registry.sources.contains_key(name) => Ok(vec![name.to_owned()]),
        Some(name) => Err(format!("unknown source {name:?}").into()),
        None => Ok(registry.sources.keys().cloned().collect()),
    }
}

fn load_registry(project_dir: &Path) -> Result<(Option<PathBuf>, SourceRegistry), Box<dyn Error>> {
    let directory = project_config_dir(project_dir, false)?;
    let registry = match &directory {
        Some(directory) => SourceRegistry::load(&directory.join("sources.toml"))?,
        None => SourceRegistry::default(),
    };
    Ok((directory, registry))
}

fn save_registry(project_dir: &Path, registry: &SourceRegistry) -> Result<(), Box<dyn Error>> {
    let directory = project_config_dir(project_dir, true)?
        .ok_or("could not create the project configuration directory")?;
    registry.save(&directory.join("sources.toml"))?;
    Ok(())
}

fn project_config_dir(project_dir: &Path, create: bool) -> Result<Option<PathBuf>, io::Error> {
    let path = project_dir.join(".inifra");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(Some(path)),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} must be a real directory, not a symlink or file",
                path.display()
            ),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !create => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&path)?;
            Ok(Some(path))
        }
        Err(error) => Err(error),
    }
}

fn source_cache_dir(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let root =
        crate::user_config::cache_dir()?.ok_or("could not determine the user cache directory")?;
    Ok(root.join("sources").join(name))
}
