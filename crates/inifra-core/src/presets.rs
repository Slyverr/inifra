//! User-owned template selections and their saved input values.

use crate::{catalog, inputs, workflow};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

/// A saved selection of templates and input values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preset {
    /// Directory name that identifies this preset.
    pub id: String,
    /// Human-readable summary.
    pub description: String,
    /// Template IDs to select when applying the preset.
    pub templates: Vec<String>,
    /// Saved values keyed by the corresponding template input names.
    pub values: BTreeMap<String, String>,
}

/// An error loading or validating a preset.
#[derive(Debug)]
pub struct PresetError(String);

impl fmt::Display for PresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for PresetError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetManifest {
    schema_version: u32,
    description: String,
    templates: Vec<String>,
    #[serde(default)]
    values: BTreeMap<String, String>,
}

/// Loads and validates preset directories beneath `root`.
///
/// A missing preset directory is an empty store. Each preset is stored at
/// `<root>/<preset-id>/preset.toml`; its directory name is the preset ID.
///
/// # Errors
/// Returns an error for malformed manifests, invalid IDs, unknown template references, invalid
/// saved input names, or filesystem entries that are not preset directories.
pub fn load_all(root: &Path, catalog: &catalog::Catalog) -> Result<Vec<Preset>, PresetError> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(PresetError(format!("{}: {error}", root.display()))),
    };
    if !metadata.file_type().is_dir() {
        return Err(PresetError(format!(
            "{}: preset store must be a directory",
            root.display()
        )));
    }

    let entries =
        fs::read_dir(root).map_err(|error| PresetError(format!("{}: {error}", root.display())))?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| PresetError(format!("{}: {error}", root.display())))?;
        let file_type = entry
            .file_type()
            .map_err(|error| PresetError(format!("{}: {error}", entry.path().display())))?;
        if !file_type.is_dir() {
            return Err(PresetError(format!(
                "{}: preset store entries must be directories",
                entry.path().display()
            )));
        }
        directories.push(entry.path());
    }
    directories.sort();

    directories
        .iter()
        .map(|directory| load_one(directory, catalog))
        .collect()
}

fn load_one(directory: &Path, catalog: &catalog::Catalog) -> Result<Preset, PresetError> {
    let id = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PresetError(format!("{}: invalid preset ID", directory.display())))?;
    if !catalog::valid_identifier(id) {
        return Err(PresetError(format!(
            "{}: invalid preset ID {id:?}",
            directory.display()
        )));
    }

    let path = directory.join("preset.toml");
    let text = crate::safe_fs::read_regular_file(&path, "preset manifest")
        .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
    let manifest: PresetManifest = toml::from_str(&text)
        .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
    if manifest.schema_version != 1 {
        return Err(PresetError(format!(
            "{}: unsupported preset schema version {}",
            path.display(),
            manifest.schema_version
        )));
    }
    if manifest.description.trim().is_empty()
        || manifest.description.chars().any(char::is_control)
        || manifest.templates.is_empty()
    {
        return Err(PresetError(format!(
            "{}: description and at least one template are required",
            path.display()
        )));
    }
    let mut seen = BTreeSet::new();
    for template in &manifest.templates {
        if !catalog::valid_identifier(template) || !seen.insert(template) {
            return Err(PresetError(format!(
                "{}: invalid or repeated template ID {template:?}",
                path.display()
            )));
        }
    }
    for name in manifest.values.keys() {
        if !inputs::valid_name(name) || name == "references" {
            return Err(PresetError(format!(
                "{}: invalid saved input name {name:?}",
                path.display()
            )));
        }
    }

    let definitions = workflow::input_definitions_for_selection(catalog, &manifest.templates)
        .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
    catalog
        .resolve_outputs(&manifest.templates)
        .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
    if let Some(name) = manifest
        .values
        .keys()
        .find(|name| !definitions.contains_key(*name))
    {
        return Err(PresetError(format!(
            "{}: saved value {name:?} is not used by the selected templates",
            path.display()
        )));
    }

    Ok(Preset {
        id: id.to_owned(),
        description: manifest.description,
        templates: manifest.templates,
        values: manifest.values,
    })
}

/// Returns the user-config path containing preset directories.
pub fn store_path(config_dir: &Path) -> PathBuf {
    config_dir.join("presets")
}
