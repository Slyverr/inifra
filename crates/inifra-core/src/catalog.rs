//! Discovery of categorized template packs on disk.

use include_dir::{Dir, include_dir};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

static BUNDLED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../templates");

/// Extracts the compile-time bundled catalog into an existing private directory.
///
/// # Errors
/// Returns an error when catalog files cannot be written.
pub fn extract_bundled(destination: &Path) -> Result<(), std::io::Error> {
    BUNDLED.extract(destination)
}

/// A discovered template and its location in the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// Globally unique pack identifier.
    pub id: String,
    /// Category directory name.
    pub category: String,
    /// Human-readable summary.
    pub description: String,
    /// Pack directory containing its sources.
    pub directory: PathBuf,
    /// Names of catalog-wide inputs used by this pack.
    pub inputs: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    description: String,
    #[serde(default)]
    inputs: Vec<String>,
}

/// A catalog discovery or validation error.
#[derive(Debug)]
pub struct CatalogError(String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for CatalogError {}

/// Discovers packs in category directories, ordered by category and ID.
///
/// # Errors
/// Returns an error for invalid IDs, duplicate IDs, unreadable manifests, or unsupported schemas.
pub fn discover(root: &Path) -> Result<Vec<Pack>, CatalogError> {
    let mut packs = Vec::new();
    let mut ids = BTreeSet::new();
    for category in directories(root)? {
        let category_id = identifier(&category)?;
        for dir in directories(&category)? {
            let id = identifier(&dir)?;
            if !ids.insert(id.clone()) {
                return Err(CatalogError(format!("duplicate template ID {id:?}")));
            }
            let path = dir.join("template.toml");
            let text = fs::read_to_string(&path)
                .map_err(|e| CatalogError(format!("{}: {e}", path.display())))?;
            let manifest: Manifest = toml::from_str(&text)
                .map_err(|e| CatalogError(format!("{}: {e}", path.display())))?;
            if manifest.schema_version != 2 {
                return Err(CatalogError(format!(
                    "{}: unsupported schema version {}",
                    path.display(),
                    manifest.schema_version
                )));
            }
            if manifest.description.trim().is_empty() {
                return Err(CatalogError(format!(
                    "{}: description is empty",
                    path.display()
                )));
            }
            packs.push(Pack {
                id,
                category: category_id.clone(),
                description: manifest.description,
                directory: dir,
                inputs: manifest.inputs,
            });
        }
    }
    Ok(packs)
}

fn directories(path: &Path) -> Result<Vec<PathBuf>, CatalogError> {
    let entries =
        fs::read_dir(path).map_err(|e| CatalogError(format!("{}: {e}", path.display())))?;
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| CatalogError(format!("{}: {e}", path.display())))?;
        let kind = entry
            .file_type()
            .map_err(|e| CatalogError(format!("{}: {e}", entry.path().display())))?;
        if kind.is_symlink() {
            return Err(CatalogError(format!(
                "{}: symlinks are not supported",
                entry.path().display()
            )));
        }
        if kind.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn identifier(path: &Path) -> Result<String, CatalogError> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(CatalogError(format!("{}: invalid ID", path.display())));
    }
    Ok(name.to_owned())
}
