use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use super::builtin::PACKS;
use super::manifest;
use super::model::{Template, TemplateFile};

pub(super) struct EmbeddedPack {
    pub(super) manifest: &'static str,
    pub(super) files: &'static [EmbeddedFile],
}

pub(super) struct EmbeddedFile {
    pub(super) source: &'static str,
    pub(super) contents: &'static [u8],
}

pub(super) fn discover_builtins() -> Result<Vec<Template>> {
    let mut catalog = BTreeMap::new();

    for pack in PACKS {
        let manifest =
            manifest::parse(pack.manifest).context("invalid built-in template manifest")?;
        let mut files = Vec::with_capacity(manifest.files.len());

        for file in &manifest.files {
            let embedded = pack
                .files
                .iter()
                .find(|embedded| embedded.source == file.source)
                .with_context(|| {
                    format!(
                        "template {:?} refers to missing embedded source {:?}",
                        manifest.id, file.source
                    )
                })?;
            files.push(TemplateFile {
                path: PathBuf::from(&file.output),
                contents: embedded.contents.to_vec(),
            });
        }

        for embedded in pack.files {
            if !manifest
                .files
                .iter()
                .any(|file| file.source == embedded.source)
            {
                bail!(
                    "built-in template {:?} embeds an unlisted source {:?}",
                    manifest.id,
                    embedded.source
                );
            }
        }

        let id = manifest.id;
        let template = Template {
            id: id.clone(),
            description: manifest.description,
            files,
        };
        if catalog.insert(id.clone(), template).is_some() {
            bail!("duplicate built-in template ID: {id}");
        }
    }

    Ok(catalog.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_manifest_resolves_all_embedded_files() {
        let templates = discover_builtins().expect("load built-in templates");

        assert_eq!(templates.len(), PACKS.len());
        assert!(templates.iter().all(|template| !template.files.is_empty()));
    }
}
