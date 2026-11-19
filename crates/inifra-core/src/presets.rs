//! User-owned template selections and their saved input values.

use crate::{catalog, inputs, workflow};
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize, Serialize)]
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
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(PresetError(format!(
                "{}: preset must be a regular directory",
                directory.display()
            )));
        }
        Err(error) => return Err(PresetError(format!("{}: {error}", directory.display()))),
    }
    let id = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PresetError(format!("{}: invalid preset ID", directory.display())))?;
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
    let preset = Preset {
        id: id.to_owned(),
        description: manifest.description,
        templates: manifest.templates,
        values: manifest.values,
    };
    validate_preset(&preset, catalog)
        .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
    Ok(preset)
}

/// Saves a validated preset to its user-owned directory.
///
/// # Errors
/// Returns an error for an invalid preset, an existing preset without `overwrite`, unsafe
/// filesystem entries, or a failed write.
pub fn save(
    root: &Path,
    preset: &Preset,
    catalog: &catalog::Catalog,
    overwrite: bool,
) -> Result<(), PresetError> {
    validate_preset(preset, catalog)?;
    match fs::symlink_metadata(root) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(PresetError(format!(
                "{}: preset store must be a regular directory",
                root.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(root)
                .map_err(|error| PresetError(format!("{}: {error}", root.display())))?;
        }
        Err(error) => return Err(PresetError(format!("{}: {error}", root.display()))),
    }

    let directory = root.join(&preset.id);
    let existed = match fs::symlink_metadata(&directory) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(PresetError(format!(
                "{}: preset must be a regular directory",
                directory.display()
            )));
        }
        Ok(_) if !overwrite => {
            return Err(PresetError(format!(
                "preset {:?} already exists; pass overwrite to replace it",
                preset.id
            )));
        }
        Ok(_) => {
            validate_managed_directory(&directory)?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(PresetError(format!("{}: {error}", directory.display()))),
    };

    if !existed {
        fs::create_dir(&directory)
            .map_err(|error| PresetError(format!("{}: {error}", directory.display())))?;
    }
    let path = directory.join("preset.toml");
    if let Err(error) = write_manifest(&path, preset) {
        if !existed && let Err(cleanup_error) = fs::remove_dir(&directory) {
            return Err(PresetError(format!(
                "{error}; removing incomplete preset also failed: {cleanup_error}"
            )));
        }
        return Err(PresetError(format!("{}: {error}", path.display())));
    }
    Ok(())
}

/// Renames a saved preset directory without changing its contents.
///
/// # Errors
/// Returns an error when either ID is invalid, the source preset is missing or invalid, the
/// destination already exists, or the filesystem rename fails.
pub fn rename(
    root: &Path,
    old_id: &str,
    new_id: &str,
    catalog: &catalog::Catalog,
) -> Result<(), PresetError> {
    validate_identifier(old_id)?;
    validate_identifier(new_id)?;
    if old_id == new_id {
        return Err(PresetError(format!(
            "new preset ID is the same as the current ID {old_id:?}"
        )));
    }
    validate_store_directory(root)?;
    let source = root.join(old_id);
    validate_managed_directory(&source)?;
    let preset = load_one(&source, catalog)?;
    let mut renamed = preset;
    renamed.id = new_id.to_owned();
    validate_preset(&renamed, catalog)?;
    let destination = root.join(new_id);
    ensure_missing(&destination)?;
    fs::rename(&source, &destination)
        .map_err(|error| PresetError(format!("could not rename preset {old_id:?}: {error}")))
}

/// Removes a saved preset directory, including one with a malformed manifest, for recovery.
///
/// Additional files in a preset directory are preserved by refusing removal; the preset format
/// owns only `preset.toml`. Its contents need not parse, so a broken preset can be removed.
///
/// # Errors
/// Returns an error when the directory contains unexpected data, is unsafe, or cannot be removed.
pub fn remove(root: &Path, id: &str) -> Result<(), PresetError> {
    validate_identifier(id)?;
    validate_store_directory(root)?;
    let directory = root.join(id);
    validate_managed_directory(&directory)?;
    fs::remove_dir_all(&directory)
        .map_err(|error| PresetError(format!("{}: {error}", directory.display())))
}

fn validate_store_directory(root: &Path) -> Result<(), PresetError> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(PresetError(format!(
            "{}: preset store must be a regular directory",
            root.display()
        ))),
        Err(error) => Err(PresetError(format!("{}: {error}", root.display()))),
    }
}

fn validate_managed_directory(directory: &Path) -> Result<(), PresetError> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(PresetError(format!(
                "{}: preset must be a regular directory",
                directory.display()
            )));
        }
        Err(error) => return Err(PresetError(format!("{}: {error}", directory.display()))),
    }
    for entry in fs::read_dir(directory)
        .map_err(|error| PresetError(format!("{}: {error}", directory.display())))?
    {
        let entry =
            entry.map_err(|error| PresetError(format!("{}: {error}", directory.display())))?;
        let kind = entry
            .file_type()
            .map_err(|error| PresetError(format!("{}: {error}", entry.path().display())))?;
        if entry.file_name() != "preset.toml" || !kind.is_file() {
            return Err(PresetError(format!(
                "{}: refusing to change unexpected preset data",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

/// Returns IDs of saved presets that select the given template.
///
/// # Errors
/// Returns an error if the preset store contains invalid data.
pub fn templates_referencing(
    root: &Path,
    catalog: &catalog::Catalog,
    template_id: &str,
) -> Result<Vec<String>, PresetError> {
    Ok(load_all(root, catalog)?
        .into_iter()
        .filter(|preset| preset.templates.iter().any(|id| id == template_id))
        .map(|preset| preset.id)
        .collect())
}

/// Rewrites template IDs in saved preset manifests, rolling back earlier files on failure.
///
/// The caller should validate the updated store against the renamed catalog and call this with
/// the IDs reversed if later catalog validation fails.
///
/// # Errors
/// Returns an error for malformed manifests or failed writes. Files already updated are restored
/// to their original contents when a later write fails.
pub fn rename_template_references(
    root: &Path,
    old_id: &str,
    new_id: &str,
) -> Result<usize, PresetError> {
    validate_identifier(old_id)?;
    validate_identifier(new_id)?;
    let presets = load_manifest_paths(root)?;
    let mut updates = Vec::new();
    for path in presets {
        let text = crate::safe_fs::read_regular_file(&path, "preset manifest")
            .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
        let mut manifest: PresetManifest = toml::from_str(&text)
            .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
        if manifest.schema_version != 1 {
            return Err(PresetError(format!(
                "{}: unsupported preset schema version {}",
                path.display(),
                manifest.schema_version
            )));
        }
        if manifest.templates.iter().any(|id| id == old_id) {
            for template in &mut manifest.templates {
                if template == old_id {
                    *template = new_id.to_owned();
                }
            }
            let updated = toml::to_string_pretty(&manifest)
                .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
            updates.push((path, text, updated));
        }
    }

    let mut written = Vec::new();
    for (path, original, updated) in &updates {
        if let Err(error) = crate::safe_fs::write_atomic(path, updated.as_bytes()) {
            let rollback_errors = rollback_manifests(&written);
            let message = if rollback_errors.is_empty() {
                format!("{}: {error}", path.display())
            } else {
                format!(
                    "{}: {error}; restoring earlier preset changes also failed: {}",
                    path.display(),
                    rollback_errors.join("; ")
                )
            };
            return Err(PresetError(message));
        }
        written.push((path.clone(), original.clone()));
    }
    Ok(updates.len())
}

fn rollback_manifests(written: &[(PathBuf, String)]) -> Vec<String> {
    written
        .iter()
        .filter_map(|(path, contents)| {
            crate::safe_fs::write_atomic(path, contents.as_bytes())
                .err()
                .map(|error| format!("{}: {error}", path.display()))
        })
        .collect()
}

fn load_manifest_paths(root: &Path) -> Result<Vec<PathBuf>, PresetError> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(PresetError(format!(
                "{}: preset store must be a regular directory",
                root.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(PresetError(format!("{}: {error}", root.display()))),
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => return Err(PresetError(format!("{}: {error}", root.display()))),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| PresetError(format!("{}: {error}", root.display())))?;
        let kind = entry
            .file_type()
            .map_err(|error| PresetError(format!("{}: {error}", entry.path().display())))?;
        if !kind.is_dir() {
            return Err(PresetError(format!(
                "{}: preset store entries must be directories",
                entry.path().display()
            )));
        }
        paths.push(entry.path().join("preset.toml"));
    }
    paths.sort();
    Ok(paths)
}

fn validate_identifier(id: &str) -> Result<(), PresetError> {
    if catalog::valid_identifier(id) {
        Ok(())
    } else {
        Err(PresetError(format!(
            "invalid preset ID {id:?}; use lowercase letters, digits, and hyphens"
        )))
    }
}

fn ensure_missing(path: &Path) -> Result<(), PresetError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(PresetError(format!(
            "preset destination already exists: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PresetError(format!("{}: {error}", path.display()))),
    }
}

fn validate_preset(preset: &Preset, catalog: &catalog::Catalog) -> Result<(), PresetError> {
    validate_identifier(&preset.id)?;
    if preset.description.trim().is_empty()
        || preset.description.chars().any(char::is_control)
        || preset.templates.is_empty()
    {
        return Err(PresetError(
            "description and at least one template are required".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    for template in &preset.templates {
        if !catalog::valid_template_id(template) || !seen.insert(template) {
            return Err(PresetError(format!(
                "invalid or repeated template ID {template:?}"
            )));
        }
    }
    for name in preset.values.keys() {
        if !inputs::valid_name(name) || name == "references" {
            return Err(PresetError(format!("invalid saved input name {name:?}")));
        }
    }
    let definitions = workflow::input_definitions_for_selection(catalog, &preset.templates)
        .map_err(|error| PresetError(error.to_string()))?;
    catalog
        .resolve_outputs(&preset.templates)
        .map_err(|error| PresetError(error.to_string()))?;
    if let Some(name) = preset
        .values
        .keys()
        .find(|name| !definitions.contains_key(*name))
    {
        return Err(PresetError(format!(
            "saved value {name:?} is not used by the selected templates"
        )));
    }
    Ok(())
}

fn write_manifest(path: &Path, preset: &Preset) -> Result<(), PresetError> {
    let manifest = PresetManifest {
        schema_version: 1,
        description: preset.description.clone(),
        templates: preset.templates.clone(),
        values: preset.values.clone(),
    };
    let text = toml::to_string_pretty(&manifest)
        .map_err(|error| PresetError(format!("{}: {error}", path.display())))?;
    crate::safe_fs::write_atomic(path, text.as_bytes())
        .map_err(|error| PresetError(format!("{}: {error}", path.display())))
}

/// Returns the user-config path containing preset directories.
pub fn store_path(config_dir: &Path) -> PathBuf {
    config_dir.join("presets")
}
