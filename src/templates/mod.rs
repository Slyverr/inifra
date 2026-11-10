mod builtin;
mod catalog;
mod inputs;
mod local;
mod manifest;
mod model;
mod render;

pub(crate) use inputs::{parse_assignments, resolve_values};
pub(crate) use model::TemplateFile;
pub(crate) use model::{Template, TemplateOrigin};
pub(crate) use render::render_text_files;

pub(crate) fn discover() -> anyhow::Result<Vec<Template>> {
    let local_root = crate::paths::config_root()?
        .join("inifra")
        .join("templates");
    catalog::discover(Some(&local_root))
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
