use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::manifest;
use super::model::{Template, TemplateFile, TemplateOrigin};

pub(super) fn discover_at(root: Option<&Path>) -> Result<Vec<Template>> {
    let Some(root) = root else {
        return Ok(Vec::new());
    };

    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!(
                "local template path must be a regular directory: {}",
                root.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "could not inspect local template directory {}",
                    root.display()
                )
            });
        }
    }

    let mut pack_dirs = Vec::new();
    for entry in fs::read_dir(root)
        .with_context(|| format!("could not read local template directory {}", root.display()))?
    {
        let entry =
            entry.with_context(|| format!("could not read an entry in {}", root.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("could not inspect {}", entry.path().display()))?;
        if file_type.is_symlink() {
            bail!(
                "symlinks are not allowed in the local template directory: {}",
                entry.path().display()
            );
        }
        if file_type.is_dir() {
            pack_dirs.push(entry.path());
        } else if !file_type.is_file() {
            bail!(
                "unsupported entry in local template directory: {}",
                entry.path().display()
            );
        }
    }
    pack_dirs.sort();

    let mut ids = HashSet::with_capacity(pack_dirs.len());
    let mut templates = Vec::with_capacity(pack_dirs.len());
    for pack_dir in pack_dirs {
        validate_pack_tree(&pack_dir)?;
        let template = load_pack(&pack_dir)?;
        if !ids.insert(template.id.clone()) {
            bail!("multiple local templates use the same ID: {}", template.id);
        }
        templates.push(template);
    }

    Ok(templates)
}

fn validate_pack_tree(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("could not read template pack {}", directory.display()))?
    {
        let entry =
            entry.with_context(|| format!("could not read an entry in {}", directory.display()))?;
        let file_type = entry.file_type().with_context(|| {
            format!(
                "could not inspect template pack entry {}",
                entry.path().display()
            )
        })?;
        if file_type.is_symlink() || (!file_type.is_file() && !file_type.is_dir()) {
            bail!(
                "template packs may contain only regular files and directories: {}",
                entry.path().display()
            );
        }
        if file_type.is_dir() {
            validate_pack_tree(&entry.path())?;
        }
    }
    Ok(())
}

fn load_pack(root: &Path) -> Result<Template> {
    let manifest_path = root.join("template.toml");
    let metadata = fs::symlink_metadata(&manifest_path).with_context(|| {
        format!(
            "template pack {} must contain template.toml",
            root.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "template manifest must be a regular file: {}",
            manifest_path.display()
        );
    }

    let source = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "could not read template manifest {}",
            manifest_path.display()
        )
    })?;
    let manifest = manifest::parse(&source)
        .with_context(|| format!("invalid template manifest {}", manifest_path.display()))?;

    let mut files = Vec::with_capacity(manifest.files.len());
    for file in manifest.files {
        let source_path = read_source(root, &file.source)?;
        files.push(TemplateFile {
            path: PathBuf::from(file.output),
            contents: source_path,
            render_text: file.source.ends_with(".j2"),
        });
    }

    Ok(Template {
        id: manifest.id,
        description: manifest.description,
        category: manifest.category,
        origin: TemplateOrigin::Local,
        files,
    })
}

fn read_source(root: &Path, source: &str) -> Result<Vec<u8>> {
    let relative = Path::new(source);
    let components = relative.components().collect::<Vec<_>>();
    let mut current = root.to_path_buf();

    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            bail!("unsafe template source path: {source:?}");
        };
        current.push(name);
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("could not inspect template source {}", current.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "symlinks are not allowed in template sources: {}",
                current.display()
            );
        }
        let is_last = index + 1 == components.len();
        if (is_last && !metadata.is_file()) || (!is_last && !metadata.is_dir()) {
            bail!("invalid template source path: {}", current.display());
        }
    }

    fs::read(&current)
        .with_context(|| format!("could not read template source {}", current.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_even_when_the_manifest_does_not_reference_them() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().expect("create parent directory");
        let root = parent.path().join("templates");
        fs::create_dir(&root).expect("create template root");
        let pack = root.join("notes");
        fs::create_dir(&pack).expect("create template pack");
        let outside = tempfile::tempdir().expect("create outside directory");
        symlink(outside.path(), pack.join("unused-link")).expect("create symlink");

        assert!(discover_at(Some(&root)).is_err());
    }
}
