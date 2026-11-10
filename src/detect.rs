use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub(crate) fn project_inputs(directory: &Path) -> Result<BTreeMap<String, String>> {
    let mut detected = BTreeMap::new();
    detect_cargo_manifest(directory, &mut detected)?;
    detect_package_json(directory, &mut detected)?;
    Ok(detected)
}

fn detect_cargo_manifest(directory: &Path, detected: &mut BTreeMap<String, String>) -> Result<()> {
    let path = directory.join("Cargo.toml");
    let Some(source) = read_manifest(&path)? else {
        return Ok(());
    };
    let manifest: toml::Value = toml::from_str(&source)
        .with_context(|| format!("could not parse project manifest {}", path.display()))?;
    let Some(package) = manifest.get("package") else {
        return Ok(());
    };
    for (input, field) in [
        ("project_name", "name"),
        ("project_description", "description"),
        ("repository_url", "repository"),
    ] {
        insert_if_missing(
            detected,
            input,
            package.get(field).and_then(toml::Value::as_str),
        );
    }
    Ok(())
}

fn detect_package_json(directory: &Path, detected: &mut BTreeMap<String, String>) -> Result<()> {
    let path = directory.join("package.json");
    let Some(source) = read_manifest(&path)? else {
        return Ok(());
    };
    let manifest: serde_json::Value = serde_json::from_str(&source)
        .with_context(|| format!("could not parse project manifest {}", path.display()))?;
    for (input, field) in [
        ("project_name", "name"),
        ("project_description", "description"),
    ] {
        insert_if_missing(
            detected,
            input,
            manifest.get(field).and_then(serde_json::Value::as_str),
        );
    }
    if let Some(repository) = manifest.get("repository").and_then(|repository| {
        repository
            .as_str()
            .or_else(|| repository.get("url").and_then(serde_json::Value::as_str))
    }) {
        insert_if_missing(detected, "repository_url", Some(repository));
    }
    Ok(())
}

fn insert_if_missing(detected: &mut BTreeMap<String, String>, input: &str, value: Option<&str>) {
    if !detected.contains_key(input)
        && let Some(value) = value.filter(|value| !value.trim().is_empty())
    {
        detected.insert(input.to_owned(), value.to_owned());
    }
}

fn read_manifest(path: &Path) -> Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("could not inspect project manifest {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "project manifest must be a regular file: {}",
            path.display()
        );
    }
    fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("could not read project manifest {}", path.display()))
}
