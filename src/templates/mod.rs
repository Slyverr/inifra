mod builtin;
mod catalog;
mod inputs;
mod local;
mod manifest;
mod model;
mod render;
mod search;
mod sources;

use anyhow::{Context, Result};

pub(crate) use inputs::{parse_assignments, resolve_values, validation_values};
pub(crate) use model::TemplateFile;
pub(crate) use model::{Template, TemplateOrigin};
pub(crate) use render::render_text_files;
pub(crate) use search::TemplateQuery;
pub(crate) use sources::SourceRegistry;

pub(crate) fn discover_for(project_directory: Option<&std::path::Path>) -> Result<Vec<Template>> {
    let local_root = crate::paths::config_root()?
        .join("inifra")
        .join("templates");
    let Some(project_directory) = project_directory else {
        return catalog::discover(Some(&local_root));
    };
    let project_root = project_templates_root(project_directory)?;
    let mut templates = catalog::discover_with_project(Some(&local_root), Some(&project_root))?;
    templates.extend(sources::discover_project_sources(project_directory)?);
    Ok(templates)
}

pub(crate) use sources::{
    compare_cached_templates, fetch_source, read_project_registry, source_is_cached,
    sync_project_sources, write_project_registry,
};

fn project_templates_root(project_directory: &std::path::Path) -> Result<std::path::PathBuf> {
    match std::fs::metadata(project_directory) {
        Ok(metadata) if !metadata.is_dir() => {
            anyhow::bail!(
                "project path must be a directory: {}",
                project_directory.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(project_directory.join(".inifra/templates"));
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "could not inspect project directory {}",
                    project_directory.display()
                )
            });
        }
    }
    let project_config = project_directory.join(".inifra");
    match std::fs::symlink_metadata(&project_config) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            anyhow::bail!(
                "project Inifra config path must be a regular directory: {}",
                project_config.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "could not inspect project Inifra config path {}",
                    project_config.display()
                )
            });
        }
    }
    Ok(project_config.join("templates"))
}

pub(crate) fn project_pack_path(
    project_directory: &std::path::Path,
    id: &str,
) -> Result<std::path::PathBuf> {
    let root = project_templates_root(project_directory)?;
    match std::fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            anyhow::bail!(
                "project template directory must be a regular directory: {}",
                root.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "could not inspect project template directory {}",
                    root.display()
                )
            });
        }
    }
    Ok(root.join(id))
}

pub(crate) fn validate_id(id: &str) -> anyhow::Result<()> {
    manifest::validate_id(id)
}

pub(crate) fn local_pack_path(id: &str) -> anyhow::Result<std::path::PathBuf> {
    crate::paths::config_root().map(|path| path.join("inifra").join("templates").join(id))
}

pub(crate) fn copy_builtin(id: &str, destination: &std::path::Path) -> anyhow::Result<()> {
    catalog::copy_builtin(id, destination)
}

pub(crate) fn source_files(pack: &std::path::Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    local::source_files(pack)
}
