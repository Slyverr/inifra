//! Catalog-wide input definitions and value validation.

use crate::safe_fs;
use serde::Deserialize;
use std::{collections::BTreeMap, error::Error, fmt, fs, path::Path};

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
    let text = safe_fs::read_regular_file(path, "input definitions")
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
    validate_definitions(&file.inputs, path)?;
    Ok(file.inputs)
}

/// Loads optional shared definitions, returning an empty map when the file is absent.
///
/// # Errors
/// Returns an error if the file exists but is unreadable, invalid, or not a regular file.
pub fn load_optional(path: &Path) -> Result<BTreeMap<String, InputDefinition>, InputError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(InputError(format!("{}: {error}", path.display()))),
        Ok(_) => load(path),
    }
}

pub(crate) fn validate_definitions(
    definitions: &BTreeMap<String, InputDefinition>,
    path: &Path,
) -> Result<(), InputError> {
    for (name, definition) in definitions {
        if !valid_name(name) || name == "references" || definition.prompt.trim().is_empty() {
            return Err(InputError(format!(
                "{}: invalid definition {name:?}",
                path.display()
            )));
        }
    }
    Ok(())
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
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
