//! Catalog-wide input definitions and value validation.

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use serde::Deserialize;
use std::{collections::BTreeMap, error::Error, fmt, io::Read, path::Path};

/// A single value requested by a template.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InputDefinition {
    /// Text shown when asking for the value.
    pub prompt: String,
    /// Whether an empty or absent value is rejected.
    #[serde(default)]
    pub required: bool,
    /// Optional value used when none is supplied.
    pub default: Option<String>,
}

/// An invalid input definition or supplied value.
#[derive(Debug)]
pub struct InputError(String);

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for InputError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputFile {
    schema_version: u32,
    #[serde(default)]
    inputs: BTreeMap<String, InputDefinition>,
}

/// Loads shared definitions from a catalog's `inputs.toml`.
///
/// # Errors
/// Returns an error if the file is unreadable, invalid, or has an unsupported schema.
pub fn load(path: &Path) -> Result<BTreeMap<String, InputDefinition>, InputError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .ok_or_else(|| InputError(format!("{}: expected a file path", path.display())))?;
    let directory = Dir::open_ambient_dir(parent, ambient_authority())
        .map_err(|e| InputError(format!("{}: {e}", path.display())))?;
    let metadata = directory
        .symlink_metadata(filename)
        .map_err(|e| InputError(format!("{}: {e}", path.display())))?;
    if !metadata.file_type().is_file() {
        return Err(InputError(format!(
            "{}: input definitions must be a regular file",
            path.display()
        )));
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut input = directory
        .open_with(filename, &options)
        .map_err(|e| InputError(format!("{}: {e}", path.display())))?;
    let mut text = String::new();
    input
        .read_to_string(&mut text)
        .map_err(|e| InputError(format!("{}: {e}", path.display())))?;
    let file: InputFile =
        toml::from_str(&text).map_err(|e| InputError(format!("{}: {e}", path.display())))?;
    if file.schema_version != 1 {
        return Err(InputError(format!(
            "{}: unsupported input schema {}",
            path.display(),
            file.schema_version
        )));
    }
    for (name, definition) in &file.inputs {
        if name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
            || definition.prompt.trim().is_empty()
        {
            return Err(InputError(format!(
                "{}: invalid definition {name:?}",
                path.display()
            )));
        }
    }
    Ok(file.inputs)
}

/// Resolves supplied values and defaults against the declared inputs.
///
/// # Errors
/// Returns an error for undeclared values or missing required inputs.
pub fn resolve(
    definitions: &BTreeMap<String, InputDefinition>,
    supplied: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, InputError> {
    for name in supplied.keys() {
        if !definitions.contains_key(name) {
            return Err(InputError(format!("undeclared input {name:?}")));
        }
    }
    let mut values = BTreeMap::new();
    for (name, definition) in definitions {
        let value = supplied.get(name).or(definition.default.as_ref());
        if definition.required && value.is_none_or(|v| v.trim().is_empty()) {
            return Err(InputError(format!("input {name:?} is required")));
        }
        values.insert(name.clone(), value.cloned().unwrap_or_default());
    }
    Ok(values)
}
