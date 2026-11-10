use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    schema_version: u32,
    pub(super) id: String,
    pub(super) description: String,
    #[serde(default)]
    pub(super) category: Option<String>,
    pub(super) files: Vec<ManifestFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestFile {
    pub(super) source: String,
    pub(super) output: String,
}

pub(super) fn parse(source: &str) -> Result<Manifest> {
    let manifest: Manifest = toml::from_str(source).context("could not parse template manifest")?;
    validate(&manifest)?;
    Ok(manifest)
}

fn validate(manifest: &Manifest) -> Result<()> {
    if manifest.schema_version != 1 {
        bail!(
            "unsupported template manifest schema version {}; supported version is 1",
            manifest.schema_version
        );
    }
    validate_id(&manifest.id)?;
    if manifest.description.trim().is_empty() {
        bail!("template {} must have a description", manifest.id);
    }
    if manifest
        .category
        .as_ref()
        .is_some_and(|category| category.trim().is_empty())
    {
        bail!("template {} must not have an empty category", manifest.id);
    }
    if manifest.files.is_empty() {
        bail!("template {} must declare at least one file", manifest.id);
    }

    let mut output_paths = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        validate_relative_path("source", &file.source)?;
        let output = validate_relative_path("output", &file.output)?;
        if output_paths.iter().any(|existing: &std::path::PathBuf| {
            existing.starts_with(&output) || output.starts_with(existing)
        }) {
            bail!(
                "template {} declares conflicting output paths near {}",
                manifest.id,
                file.output
            );
        }
        output_paths.push(output);
    }

    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.starts_with('-')
        || id.ends_with('-')
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("template ID must use lower-case letters, digits, and hyphens: {id:?}");
    }
    Ok(())
}

fn validate_relative_path(label: &str, value: &str) -> Result<std::path::PathBuf> {
    let path = Path::new(value);
    if value.contains('\0')
        || path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!(
            "template {label} path must be a non-empty relative path without traversal: {value:?}"
        );
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
schema_version = 1
id = "notes"
description = "Project notes"

[[files]]
source = "NOTES.md"
output = "docs/NOTES.md"
"#;

    #[test]
    fn accepts_a_valid_manifest() {
        assert!(parse(BASE).is_ok());
    }

    #[test]
    fn accepts_optional_category_metadata() {
        let source = BASE.replace(
            "description = \"Project notes\"",
            "description = \"Project notes\"\ncategory = \"Project guidance\"",
        );

        assert_eq!(
            parse(&source)
                .expect("parse categorized pack")
                .category
                .as_deref(),
            Some("Project guidance")
        );
    }

    #[test]
    fn rejects_unknown_schema_versions() {
        let source = BASE.replace("schema_version = 1", "schema_version = 2");
        assert!(parse(&source).is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        let source = BASE.replace(
            "description = \"Project notes\"",
            "description = \"Project notes\"\nextra = true",
        );
        assert!(parse(&source).is_err());
    }

    #[test]
    fn rejects_unsafe_source_and_output_paths() {
        let unsafe_source = BASE.replace("source = \"NOTES.md\"", "source = \"../outside\"");
        assert!(parse(&unsafe_source).is_err());

        let unsafe_output = BASE.replace("output = \"docs/NOTES.md\"", "output = \"/absolute\"");
        assert!(parse(&unsafe_output).is_err());
    }

    #[test]
    fn rejects_output_path_collisions() {
        let source = format!(
            "{BASE}\n[[files]]\nsource = \"nested.md\"\noutput = \"docs/NOTES.md/child\"\n"
        );
        assert!(parse(&source).is_err());
    }
}
