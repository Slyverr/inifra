use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

const SOURCE_FILE: &str = "sources.toml";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceRegistry {
    schema_version: u32,
    #[serde(default)]
    pub(crate) sources: BTreeMap<String, ProjectSource>,
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            sources: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectSource {
    pub(crate) url: String,
    pub(crate) revision: String,
    #[serde(default = "default_directory")]
    pub(crate) directory: String,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct CacheIdentity {
    name: String,
    url: String,
    revision: String,
    directory: String,
}

impl CacheIdentity {
    pub(super) fn new(name: &str, source: &ProjectSource) -> Self {
        Self {
            name: name.to_owned(),
            url: source.url.clone(),
            revision: source.revision.clone(),
            directory: source.directory.clone(),
        }
    }

    pub(super) fn parse(contents: &str) -> Result<Self> {
        toml::from_str(contents).context("could not parse source cache identity")
    }

    pub(super) fn serialize(&self) -> Result<String> {
        toml::to_string_pretty(self).context("could not serialize source cache identity")
    }
}

fn default_directory() -> String {
    "templates".to_owned()
}

impl SourceRegistry {
    pub(crate) fn parse(source: &str) -> Result<Self> {
        let registry: Self = toml::from_str(source).context("could not parse source registry")?;
        if registry.schema_version != 1 {
            bail!(
                "unsupported source registry schema version {}; supported version is 1",
                registry.schema_version
            );
        }
        for (name, source) in &registry.sources {
            crate::templates::validate_id(name)
                .with_context(|| format!("invalid source name {name:?}"))?;
            validate_url(&source.url)?;
            validate_revision(&source.revision)?;
            validate_directory(&source.directory)?;
        }
        Ok(registry)
    }

    pub(crate) fn serialize(&self) -> Result<String> {
        let serialized =
            toml::to_string_pretty(self).context("could not serialize source registry")?;
        Self::parse(&serialized)?;
        Ok(serialized)
    }
}

pub(crate) fn read_project_registry(project: &Path) -> Result<SourceRegistry> {
    let path = project_source_path(project, false)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            bail!("source registry must be a regular file: {}", path.display());
        }
        Ok(_) => {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("could not read source registry {}", path.display()))?;
            SourceRegistry::parse(&contents)
                .with_context(|| format!("invalid source registry {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SourceRegistry::default()),
        Err(error) => Err(error)
            .with_context(|| format!("could not inspect source registry {}", path.display())),
    }
}

pub(crate) fn write_project_registry(project: &Path, registry: &SourceRegistry) -> Result<()> {
    let path = project_source_path(project, true)?;
    let contents = registry.serialize()?;
    let parent = path
        .parent()
        .context("source registry path has no parent directory")?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("could not prepare source registry in {}", parent.display()))?;
    temporary
        .write_all(contents.as_bytes())
        .with_context(|| format!("could not write source registry {}", path.display()))?;
    temporary
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("could not save source registry {}", path.display()))?;
    Ok(())
}

fn project_source_path(project: &Path, create: bool) -> Result<PathBuf> {
    let metadata = match fs::metadata(project) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !create => {
            return Ok(project.join(".inifra").join(SOURCE_FILE));
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not inspect project directory {}", project.display())
            });
        }
    };
    if !metadata.is_dir() {
        bail!("project path must be a directory: {}", project.display());
    }
    let config = project.join(".inifra");
    match fs::symlink_metadata(&config) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!(
                "project Inifra config path must be a regular directory: {}",
                config.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
            fs::create_dir(&config).with_context(|| {
                format!(
                    "could not create project Inifra config {}",
                    config.display()
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "could not inspect project Inifra config {}",
                    config.display()
                )
            });
        }
    }
    Ok(config.join(SOURCE_FILE))
}

pub(super) fn validate_url(url: &str) -> Result<()> {
    if url.trim().is_empty()
        || url.starts_with('-')
        || url.chars().any(char::is_control)
        || (url.chars().any(char::is_whitespace) && !Path::new(url).is_absolute())
    {
        bail!(
            "source URL must be non-empty and contain no unsafe whitespace or control characters"
        );
    }
    if let Some((scheme @ ("http" | "https" | "ssh"), rest)) = url.split_once("://") {
        let authority = rest.split('/').next().unwrap_or_default();
        if ((scheme == "http" || scheme == "https") && authority.contains('@'))
            || (scheme == "https" && (rest.contains('?') || rest.contains('#')))
        {
            bail!(
                "do not embed credentials or query parameters in a source URL; use Git's configured authentication"
            );
        }
    }
    if let Some((userinfo, _)) = url.split_once('@')
        && userinfo.contains(':')
    {
        bail!("do not embed passwords in a source URL; use Git's configured authentication");
    }
    Ok(())
}

pub(super) fn validate_revision(revision: &str) -> Result<()> {
    if !matches!(revision.len(), 40 | 64) || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("source revision must be a full Git commit ID (40 or 64 hexadecimal characters)");
    }
    Ok(())
}

pub(super) fn validate_directory(directory: &str) -> Result<()> {
    let path = Path::new(directory);
    if path.as_os_str().is_empty()
        || directory.contains('\\')
        || directory.contains(':')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("source directory must be a safe relative path: {directory:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_pinned_project_source() {
        let registry = SourceRegistry::parse(
            "schema_version = 1\n\n[sources.team]\nurl = \"https://example.com/team/templates.git\"\nrevision = \"0123456789012345678901234567890123456789\"\n",
        )
        .expect("parse source registry");

        assert_eq!(registry.sources["team"].directory, "templates");
    }

    #[test]
    fn rejects_moving_refs_and_unsafe_source_directories() {
        let moving_ref = "schema_version = 1\n[sources.team]\nurl = \"https://example.com/templates.git\"\nrevision = \"main\"\n";
        let unsafe_directory = "schema_version = 1\n[sources.team]\nurl = \"https://example.com/templates.git\"\nrevision = \"0123456789012345678901234567890123456789\"\ndirectory = \"../templates\"\n";

        assert!(SourceRegistry::parse(moving_ref).is_err());
        assert!(SourceRegistry::parse(unsafe_directory).is_err());
    }
}
