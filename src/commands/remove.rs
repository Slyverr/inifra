use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::templates;
use crate::theme::{self, Tone};

pub(super) fn run(id: &str, project_directory: Option<&Path>) -> Result<()> {
    if let Some((source, _)) = id.split_once('/') {
        bail!(
            "remote template {id:?} is read-only; remove its source with `inifra source remove {source}`"
        );
    }
    templates::validate_id(id)?;

    let (pack_path, scope) = if let Some(project) = project_directory {
        (templates::project_pack_path(project, id)?, "project-local")
    } else {
        (templates::local_pack_path(id)?, "user-local")
    };
    let template_root = pack_path
        .parent()
        .context("local template path has no parent directory")?;
    let root_metadata = match fs::symlink_metadata(template_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("no {scope} template pack named {id:?} exists")
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "could not inspect template directory {}",
                    template_root.display()
                )
            });
        }
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        bail!(
            "template directory must be a regular directory: {}",
            template_root.display()
        );
    }

    let pack_metadata = match fs::symlink_metadata(&pack_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("no {scope} template pack named {id:?} exists")
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not inspect template pack {}", pack_path.display())
            });
        }
    };
    if pack_metadata.file_type().is_symlink() || !pack_metadata.is_dir() {
        bail!(
            "template pack must be a regular directory: {}",
            pack_path.display()
        );
    }

    fs::remove_dir_all(&pack_path)
        .with_context(|| format!("could not remove template pack {}", pack_path.display()))?;
    println!(
        "Removed {} template {}.",
        theme::paint_stdout(scope, Tone::Muted, false),
        theme::paint_stdout(format!("{id:?}"), Tone::Accent, true)
    );
    Ok(())
}
