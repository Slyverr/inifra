use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preset {
    schema_version: u32,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) templates: Vec<String>,
    #[serde(default)]
    pub(crate) values: BTreeMap<String, String>,
}

impl Preset {
    pub(crate) fn new(
        name: String,
        description: String,
        templates: Vec<String>,
        values: BTreeMap<String, String>,
    ) -> Self {
        Self {
            schema_version: 1,
            name,
            description,
            templates,
            values,
        }
    }
}

pub(crate) fn load(name: &str) -> Result<Preset> {
    validate_name(name)?;
    let path = preset_directory()?.join(format!("{name}.toml"));
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("could not inspect preset {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("preset must be a regular file: {}", path.display());
    }
    let source = fs::read_to_string(&path)
        .with_context(|| format!("could not read preset {}", path.display()))?;
    let preset: Preset = toml::from_str(&source)
        .with_context(|| format!("could not parse preset {}", path.display()))?;
    validate(&preset, name)?;
    Ok(preset)
}

pub(crate) fn list() -> Result<Vec<Preset>> {
    let directory = preset_directory()?;
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not read preset directory {}", directory.display())
            });
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry =
            entry.with_context(|| format!("could not read an entry in {}", directory.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("could not inspect {}", entry.path().display()))?;
        if file_type.is_symlink() || (!file_type.is_file() && !file_type.is_dir()) {
            bail!(
                "unsupported entry in preset directory: {}",
                entry.path().display()
            );
        }
        if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "toml")
        {
            paths.push(entry.path());
        }
    }
    paths.sort();

    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .context("preset filename is not valid UTF-8")?;
            load(name)
        })
        .collect()
}

pub(crate) fn save(preset: &Preset, overwrite: bool) -> Result<()> {
    validate_name(&preset.name)?;
    validate(preset, &preset.name)?;
    let directory = preset_directory()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("could not create preset directory {}", directory.display()))?;
    let metadata = fs::symlink_metadata(&directory)
        .with_context(|| format!("could not inspect preset directory {}", directory.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "preset path must be a regular directory: {}",
            directory.display()
        );
    }

    let destination = directory.join(format!("{}.toml", preset.name));
    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!(
                    "preset destination must be a regular file: {}",
                    destination.display()
                );
            }
            if !overwrite {
                bail!(
                    "preset already exists; pass --overwrite to replace it: {}",
                    destination.display()
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("could not inspect {}", destination.display()));
        }
    }

    let contents = toml::to_string_pretty(preset).context("could not serialize preset")?;
    let mut temporary = NamedTempFile::new_in(&directory).with_context(|| {
        format!(
            "could not create temporary preset in {}",
            directory.display()
        )
    })?;
    temporary.write_all(contents.as_bytes()).with_context(|| {
        format!(
            "could not write temporary preset in {}",
            directory.display()
        )
    })?;
    if overwrite {
        temporary.persist(&destination).map_err(|error| error.error)
    } else {
        temporary
            .persist_noclobber(&destination)
            .map_err(|error| error.error)
    }
    .with_context(|| format!("could not save preset {}", preset.name))?;
    Ok(())
}

fn validate(preset: &Preset, expected_name: &str) -> Result<()> {
    if preset.schema_version != 1 {
        bail!(
            "unsupported preset schema version {}; supported version is 1",
            preset.schema_version
        );
    }
    validate_name(&preset.name)?;
    if preset.name != expected_name {
        bail!(
            "preset name {:?} does not match filename {expected_name:?}",
            preset.name
        );
    }
    if preset.templates.is_empty() {
        bail!(
            "preset {:?} must include at least one template",
            preset.name
        );
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('-')
        || name.ends_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("preset name must use lower-case letters, digits, and hyphens: {name:?}");
    }
    Ok(())
}

fn preset_directory() -> Result<PathBuf> {
    crate::paths::config_root().map(|path| path.join("inifra").join("presets"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_preset_names() {
        assert!(validate_name("rust-cli").is_ok());
        assert!(validate_name("../outside").is_err());
        assert!(validate_name("Uppercase").is_err());
    }

    #[test]
    fn rejects_presets_without_templates() {
        let preset = Preset::new(
            "empty".to_owned(),
            String::new(),
            Vec::new(),
            BTreeMap::new(),
        );
        assert!(validate(&preset, "empty").is_err());
    }
}
