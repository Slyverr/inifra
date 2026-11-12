mod git;
mod registry;

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tempfile::{NamedTempFile, TempDir};

use super::local;
use super::model::{Template, TemplateOrigin};
use registry::CacheIdentity;

const CACHE_DIRECTORY: &str = "source-cache";

pub(crate) use registry::{ProjectSource, SourceRegistry};

#[derive(Debug, Default)]
pub(crate) struct TemplateChanges {
    pub(crate) added: Vec<String>,
    pub(crate) updated: Vec<String>,
    pub(crate) removed: Vec<String>,
}

pub(crate) fn compare_cached_templates(
    name: &str,
    previous: &ProjectSource,
    current: &ProjectSource,
) -> Result<Option<TemplateChanges>> {
    let Some(previous_templates) = cached_templates(name, previous)? else {
        return Ok(None);
    };
    let Some(current_templates) = cached_templates(name, current)? else {
        bail!("newly fetched source {name:?} is missing from the cache");
    };

    let previous_by_id = previous_templates
        .iter()
        .map(|template| (template.id.as_str(), template))
        .collect::<BTreeMap<_, _>>();
    let current_by_id = current_templates
        .iter()
        .map(|template| (template.id.as_str(), template))
        .collect::<BTreeMap<_, _>>();
    let mut changes = TemplateChanges::default();

    for (id, template) in &current_by_id {
        match previous_by_id.get(id) {
            None => changes.added.push((*id).to_owned()),
            Some(previous) if *previous != *template => changes.updated.push((*id).to_owned()),
            Some(_) => {}
        }
    }
    for id in previous_by_id.keys() {
        if !current_by_id.contains_key(id) {
            changes.removed.push((*id).to_owned());
        }
    }

    Ok(Some(changes))
}

fn cached_templates(name: &str, source: &ProjectSource) -> Result<Option<Vec<Template>>> {
    let path = cache_path(name, source)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let root = cached_template_root(&path, name, source)?;
            let templates = local::discover_at(Some(&root), TemplateOrigin::Remote)
                .with_context(|| format!("invalid cached template source {name:?}"))?;
            Ok(Some(templates))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("could not inspect source cache {}", path.display()))
        }
    }
}

pub(crate) fn read_project_registry(project: &Path) -> Result<SourceRegistry> {
    registry::read_project_registry(project)
}

pub(crate) fn write_project_registry(project: &Path, registry: &SourceRegistry) -> Result<()> {
    registry::write_project_registry(project, registry)
}

pub(crate) fn sync_project_sources(project: &Path) -> Result<()> {
    let registry = read_project_registry(project)?;
    for (name, source) in registry.sources {
        ensure_cached(&name, &source)?;
    }
    Ok(())
}

pub(crate) fn discover_project_sources(project: &Path) -> Result<Vec<Template>> {
    let registry = read_project_registry(project)?;
    let mut templates = Vec::new();
    for (name, source) in registry.sources {
        let cache = cache_path(&name, &source)?;
        let root = cached_template_root(&cache, &name, &source).with_context(|| {
            format!(
                "template source {name:?} is not synced; run `inifra source sync --path {}`",
                project.display()
            )
        })?;
        let mut source_templates = local::discover_at(Some(&root), TemplateOrigin::Remote)
            .with_context(|| format!("invalid template source {name:?}"))?;
        for template in &mut source_templates {
            template.id = format!("{name}/{}", template.id);
        }
        templates.extend(source_templates);
    }
    Ok(templates)
}

pub(crate) fn fetch_source(
    name: &str,
    url: &str,
    revision: &str,
    directory: &str,
) -> Result<ProjectSource> {
    super::validate_id(name)?;
    registry::validate_url(url)?;
    registry::validate_directory(directory)?;
    git::validate_fetch_reference(revision)?;

    let cache_parent = cache_root()?.join("staging");
    fs::create_dir_all(&cache_parent)
        .with_context(|| format!("could not create source cache {}", cache_parent.display()))?;
    let staging = tempfile::Builder::new()
        .prefix("source-")
        .tempdir_in(&cache_parent)
        .with_context(|| {
            format!(
                "could not create temporary source directory in {}",
                cache_parent.display()
            )
        })?;
    let source_tree = staging.path().join("templates");
    let resolved_revision = git::fetch_and_materialize(
        &staging.path().join("repository.git"),
        url,
        revision,
        directory,
        &source_tree,
    )?;

    let source = ProjectSource {
        url: url.to_owned(),
        revision: resolved_revision,
        directory: directory.to_owned(),
    };
    let templates = local::discover_at(Some(&source_tree), TemplateOrigin::Remote)
        .with_context(|| format!("source {name:?} does not contain valid template packs"))?;
    if templates.is_empty() {
        bail!(
            "source {name:?} contains no templates in {:?}",
            source.directory
        );
    }

    cache_source(staging, name, &source)?;
    Ok(source)
}

fn cache_source(staging: TempDir, name: &str, source: &ProjectSource) -> Result<()> {
    let destination = cache_path(name, source)?;
    let parent = destination
        .parent()
        .context("source cache path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("could not create source cache {}", parent.display()))?;
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!(
                "cached source path must be a regular directory: {}",
                destination.display()
            );
        }
        Ok(_) => {
            let root = cached_template_root(&destination, name, source)?;
            let templates = local::discover_at(Some(&root), TemplateOrigin::Remote)
                .with_context(|| format!("cached source {name:?} is invalid"))?;
            if templates.is_empty() {
                bail!("cached source {name:?} contains no templates");
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let identity = CacheIdentity::new(name, source);
            write_atomic(&staging.path().join("source.toml"), &identity.serialize()?)?;
            fs::remove_dir_all(staging.path().join("repository.git")).with_context(|| {
                format!("could not clean temporary Git cache for source {name:?}")
            })?;
            fs::rename(staging.path(), &destination).with_context(|| {
                format!(
                    "could not cache source {name:?} at {}",
                    destination.display()
                )
            })?;
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not inspect source cache {}", destination.display())
            });
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path.parent().context("source cache path has no parent")?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("could not prepare source metadata in {}", parent.display()))?;
    temporary
        .write_all(contents.as_bytes())
        .with_context(|| format!("could not write source metadata {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("could not save source metadata {}", path.display()))?;
    Ok(())
}

fn ensure_cached(name: &str, source: &ProjectSource) -> Result<()> {
    let path = cache_path(name, source)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!(
                "cached source must be a regular directory: {}",
                path.display()
            );
        }
        Ok(_) => {
            let root = cached_template_root(&path, name, source)?;
            let templates = local::discover_at(Some(&root), TemplateOrigin::Remote)
                .with_context(|| format!("cached source {name:?} is invalid"))?;
            if templates.is_empty() {
                bail!("cached source {name:?} contains no templates");
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let fetched = fetch_source(name, &source.url, &source.revision, &source.directory)?;
            if fetched.revision != source.revision {
                bail!(
                    "source {name:?} resolved to {}, but project pins {}",
                    fetched.revision,
                    source.revision
                );
            }
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("could not inspect cached source {}", path.display()));
        }
    }
    Ok(())
}

pub(crate) fn source_is_cached(name: &str, source: &ProjectSource) -> Result<bool> {
    let path = cache_path(name, source)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!(
                "source cache path must be a regular directory: {}",
                path.display()
            );
        }
        Ok(_) => {
            cached_template_root(&path, name, source)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("could not inspect source cache {}", path.display()))
        }
    }
}

fn cache_path(name: &str, source: &ProjectSource) -> Result<PathBuf> {
    Ok(cache_root()?.join(cache_key(name, source)))
}

fn cache_root() -> Result<PathBuf> {
    crate::paths::config_root().map(|path| path.join("inifra").join(CACHE_DIRECTORY))
}

fn cached_template_root(cache: &Path, name: &str, source: &ProjectSource) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(cache)
        .with_context(|| format!("could not inspect source cache {}", cache.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "source cache must be a regular directory: {}",
            cache.display()
        );
    }
    let identity_path = cache.join("source.toml");
    let metadata = fs::symlink_metadata(&identity_path).with_context(|| {
        format!(
            "could not inspect source cache identity {}",
            identity_path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "source cache identity must be a regular file: {}",
            identity_path.display()
        );
    }
    let contents = fs::read_to_string(&identity_path).with_context(|| {
        format!(
            "could not read source cache identity {}",
            identity_path.display()
        )
    })?;
    let identity = CacheIdentity::parse(&contents)
        .with_context(|| format!("invalid source cache identity {}", identity_path.display()))?;
    if identity != CacheIdentity::new(name, source) {
        bail!(
            "source cache identity does not match project pin: {}",
            cache.display()
        );
    }
    let root = cache.join("templates");
    match fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!(
                "cached template root must be a regular directory: {}",
                root.display()
            );
        }
        Ok(_) => Ok(root),
        Err(error) => Err(error)
            .with_context(|| format!("could not inspect cached template root {}", root.display())),
    }
}

fn cache_key(name: &str, source: &ProjectSource) -> String {
    // This hash only names cache directories; Git validates the pinned objects.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in name
        .bytes()
        .chain([0])
        .chain(source.url.bytes())
        .chain([0])
        .chain(source.revision.bytes())
        .chain([0])
        .chain(source.directory.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
