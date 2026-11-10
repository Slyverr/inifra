mod builtin;
mod catalog;
mod manifest;
mod model;

pub(crate) use model::Template;
#[cfg(test)]
pub(crate) use model::TemplateFile;

pub(crate) fn discover() -> anyhow::Result<Vec<Template>> {
    catalog::discover_builtins()
}
