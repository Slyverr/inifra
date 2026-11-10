use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::builtin::PACKS;
use super::local;
use super::manifest;
use super::model::{Template, TemplateFile, TemplateOrigin};

pub(super) struct EmbeddedPack {
    pub(super) manifest: &'static str,
    pub(super) files: &'static [EmbeddedFile],
}

pub(super) struct EmbeddedFile {
    pub(super) source: &'static str,
    pub(super) contents: &'static [u8],
}

pub(super) fn copy_builtin(id: &str, destination: &Path) -> Result<()> {
    let pack = PACKS
        .iter()
        .find(|pack| manifest::parse(pack.manifest).is_ok_and(|manifest| manifest.id == id))
        .with_context(|| format!("built-in template {id:?} was not found"))?;
    let manifest = manifest::parse(pack.manifest)?;
    fs::create_dir_all(destination)
        .with_context(|| format!("could not create template pack {}", destination.display()))?;
    fs::write(destination.join("template.toml"), pack.manifest)
        .with_context(|| format!("could not write template manifest for {id:?}"))?;
    for file in pack.files {
        let source = destination.join(file.source);
        if let Some(parent) = source.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "could not create template source directory {}",
                    parent.display()
                )
            })?;
        }
        fs::write(&source, file.contents)
            .with_context(|| format!("could not write template source {}", source.display()))?;
    }
    for file in manifest.files {
        if !pack.files.iter().any(|source| source.source == file.source) {
            bail!(
                "built-in template {id:?} refers to missing embedded source {:?}",
                file.source
            );
        }
    }
    Ok(())
}

pub(super) fn discover(local_root: Option<&Path>) -> Result<Vec<Template>> {
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
                render_text: file.source.ends_with(".j2"),
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
            category: manifest.category,
            inputs: manifest.inputs,
            origin: TemplateOrigin::Builtin,
            files,
        };
        if catalog.insert(id.clone(), template).is_some() {
            bail!("duplicate built-in template ID: {id}");
        }
    }

    for template in local::discover_at(local_root)? {
        catalog.insert(template.id.clone(), template);
    }

    Ok(catalog.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_manifest_resolves_all_embedded_files() {
        let templates = discover(None).expect("load built-in templates");

        assert_eq!(templates.len(), PACKS.len());
        assert!(templates.iter().all(|template| !template.files.is_empty()));
    }
}
