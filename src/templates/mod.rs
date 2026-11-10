mod builtin;
mod catalog;
mod local;
mod manifest;
mod model;
mod render;

#[cfg(test)]
pub(crate) use model::TemplateFile;
pub(crate) use model::{Template, TemplateOrigin};
pub(crate) use render::render_text_files;

pub(crate) fn discover() -> anyhow::Result<Vec<Template>> {
    let local_root = dirs::config_dir().map(|path| path.join("inifra").join("templates"));
    catalog::discover(local_root.as_deref())
}
