//! Project-local registry of pinned Git template sources.

use crate::safe_fs;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt, path::Path};

const SCHEMA_VERSION: u32 = 1;

/// A Git repository revision tracked by a project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitSource {
    /// Credential-free Git transport URL.
    pub url: String,
    /// Exact remote ref used by explicit update operations.
    pub reference: String,
    /// Full immutable Git commit object ID (SHA-1 or SHA-256).
    pub revision: String,
}

/// Named, project-local Git source pins.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceRegistry {
    /// Sources keyed by their stable project namespace.
    pub sources: BTreeMap<String, GitSource>,
}

/// An invalid or unreadable Git source registry.
#[derive(Debug)]
pub struct SourceRegistryError(String);

impl fmt::Display for SourceRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for SourceRegistryError {}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    schema_version: u32,
    #[serde(default)]
    sources: BTreeMap<String, GitSource>,
}

impl SourceRegistry {
    /// Loads a registry file. A missing file represents an empty registry.
    ///
    /// # Errors
    /// Returns an error for an unreadable file, malformed TOML, unsupported schema, or invalid
    /// source entry.
    pub fn load(path: &Path) -> Result<Self, SourceRegistryError> {
        let text = match safe_fs::read_regular_file(path, "source registry") {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(SourceRegistryError(format!("{}: {error}", path.display())));
            }
        };
        let file: RegistryFile = toml::from_str(&text)
            .map_err(|error| SourceRegistryError(format!("{}: {error}", path.display())))?;
        if file.schema_version != SCHEMA_VERSION {
            return Err(SourceRegistryError(format!(
                "{}: unsupported source registry schema version {}",
                path.display(),
                file.schema_version
            )));
        }
        let registry = Self {
            sources: file.sources,
        };
        registry.validate()?;
        Ok(registry)
    }

    /// Writes this registry atomically to an existing project configuration directory.
    ///
    /// # Errors
    /// Returns an error for an invalid entry, an unsafe target, or a failed write.
    pub fn save(&self, path: &Path) -> Result<(), SourceRegistryError> {
        self.validate()?;
        let file = RegistryFile {
            schema_version: SCHEMA_VERSION,
            sources: self.sources.clone(),
        };
        let text = toml::to_string_pretty(&file)
            .map_err(|error| SourceRegistryError(error.to_string()))?;
        safe_fs::write_atomic(path, text.as_bytes())
            .map_err(|error| SourceRegistryError(format!("{}: {error}", path.display())))
    }

    /// Adds a source under a stable name, rejecting an existing name.
    ///
    /// # Errors
    /// Returns an error if the name or source is invalid, or the name is already present.
    pub fn insert(&mut self, name: &str, source: GitSource) -> Result<(), SourceRegistryError> {
        validate_source_name(name)?;
        validate_git_source(&source)?;
        if self.sources.contains_key(name) {
            return Err(SourceRegistryError(format!(
                "source {name:?} already exists"
            )));
        }
        self.sources.insert(name.to_owned(), source);
        Ok(())
    }

    /// Replaces an existing source pin without changing its stable name.
    ///
    /// # Errors
    /// Returns an error if the name or source is invalid, or the name is not present.
    pub fn replace(&mut self, name: &str, source: GitSource) -> Result<(), SourceRegistryError> {
        validate_source_name(name)?;
        validate_git_source(&source)?;
        if !self.sources.contains_key(name) {
            return Err(SourceRegistryError(format!("unknown source {name:?}")));
        }
        self.sources.insert(name.to_owned(), source);
        Ok(())
    }

    /// Removes and returns a source pin.
    ///
    /// # Errors
    /// Returns an error if the name is invalid or is not present.
    pub fn remove(&mut self, name: &str) -> Result<GitSource, SourceRegistryError> {
        validate_source_name(name)?;
        self.sources
            .remove(name)
            .ok_or_else(|| SourceRegistryError(format!("unknown source {name:?}")))
    }

    /// Validates every source name and pin in the registry.
    ///
    /// # Errors
    /// Returns an error with the source name and invalid field.
    pub fn validate(&self) -> Result<(), SourceRegistryError> {
        for (name, source) in &self.sources {
            validate_source_name(name)?;
            validate_git_source(source)
                .map_err(|error| SourceRegistryError(format!("source {name:?}: {error}")))?;
        }
        Ok(())
    }
}

/// Validates a stable source name used as the namespace for remote packs.
///
/// # Errors
/// Returns an error unless the name uses lowercase ASCII letters, digits, and hyphens.
pub fn validate_source_name(name: &str) -> Result<(), SourceRegistryError> {
    if crate::catalog::valid_identifier(name) {
        Ok(())
    } else {
        Err(SourceRegistryError(format!("invalid source name {name:?}")))
    }
}

/// Validates an allowed Git transport URL without embedded HTTPS credentials.
///
/// HTTPS, SSH, scp-style SSH, and absolute `file://` URLs are supported. Authentication should
/// use Git's credential helpers or SSH agent rather than values embedded in the URL.
///
/// # Errors
/// Returns an error when the URL is empty, unsafe, unsupported, or embeds HTTPS credentials.
pub fn validate_git_url(url: &str) -> Result<(), SourceRegistryError> {
    if url.is_empty()
        || url.starts_with('-')
        || url
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(SourceRegistryError("invalid Git source URL".into()));
    }

    if let Some(authority_and_path) = url.strip_prefix("https://") {
        let authority = authority_and_path.split('/').next().unwrap_or_default();
        if authority.is_empty() || authority.contains(['@', '?', '#']) || url.contains(['?', '#']) {
            return Err(SourceRegistryError(
                "HTTPS source URLs must include a host and must not contain credentials or query fragments".into(),
            ));
        }
        return Ok(());
    }

    if let Some(authority_and_path) = url.strip_prefix("ssh://") {
        let authority = authority_and_path
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        if host.is_empty()
            || host.contains(['@', '?', '#'])
            || url.contains(['?', '#'])
            || authority
                .split_once('@')
                .is_some_and(|(user, _)| user.is_empty() || user.contains(':'))
        {
            return Err(SourceRegistryError("invalid SSH source URL".into()));
        }
        return Ok(());
    }

    if url.starts_with("file://") {
        let path = url.strip_prefix("file://").unwrap_or_default();
        if path.starts_with('/') && !url.contains(['?', '#']) {
            return Ok(());
        }
        return Err(SourceRegistryError(
            "file source URLs must use an absolute path".into(),
        ));
    }

    if let Some((user_host, path)) = url.split_once(':') {
        let valid_scp_user_host = user_host
            .split_once('@')
            .is_some_and(|(user, host)| !user.is_empty() && !host.is_empty());
        if valid_scp_user_host && !path.is_empty() && !url.contains(['?', '#']) {
            return Ok(());
        }
    }

    Err(SourceRegistryError(
        "Git source URL must use HTTPS, SSH, or an absolute file URL".into(),
    ))
}

/// Validates the full remote ref tracked for explicit updates.
///
/// `HEAD` tracks the remote's default branch; otherwise use a full `refs/heads/...` or
/// `refs/tags/...` name so branch and tag names cannot be confused.
///
/// # Errors
/// Returns an error for a ref name that Git would not accept.
pub fn validate_git_reference(reference: &str) -> Result<(), SourceRegistryError> {
    if reference == "HEAD" {
        return Ok(());
    }
    let Some(name) = reference
        .strip_prefix("refs/heads/")
        .or_else(|| reference.strip_prefix("refs/tags/"))
    else {
        return Err(SourceRegistryError(
            "Git ref must be HEAD, refs/heads/<name>, or refs/tags/<name>".into(),
        ));
    };
    let invalid = name.is_empty()
        || name.starts_with('/')
        || name.ends_with('/')
        || name.contains("//")
        || name.contains("..")
        || name.contains("@{")
        || name.chars().any(|character| {
            character.is_whitespace() || character.is_control() || " ~^:?*[\\".contains(character)
        })
        || name.split('/').any(|component| {
            component.is_empty()
                || component.starts_with('.')
                || component.ends_with('.')
                || component.ends_with(".lock")
                || component == "@"
        });
    if invalid {
        Err(SourceRegistryError(format!(
            "invalid Git ref {reference:?}"
        )))
    } else {
        Ok(())
    }
}

/// Validates a full lowercase Git commit ID (40-character SHA-1 or 64-character SHA-256).
///
/// # Errors
/// Returns an error unless the value is a full lowercase SHA-1 or SHA-256 hexadecimal ID.
pub fn validate_revision(revision: &str) -> Result<(), SourceRegistryError> {
    if matches!(revision.len(), 40 | 64)
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(SourceRegistryError(
            "Git revision must be a full lowercase 40-character SHA-1 or 64-character SHA-256 commit ID".into(),
        ))
    }
}

fn validate_git_source(source: &GitSource) -> Result<(), SourceRegistryError> {
    validate_git_url(&source.url)?;
    validate_git_reference(&source.reference)?;
    validate_revision(&source.revision)
}
