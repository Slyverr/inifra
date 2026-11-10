use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct Template {
    pub(crate) id: String,
    pub(crate) description: String,
    pub(crate) category: Option<String>,
    pub(crate) inputs: Vec<String>,
    pub(crate) origin: TemplateOrigin,
    pub(crate) files: Vec<TemplateFile>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TemplateOrigin {
    Builtin,
    Local,
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateFile {
    pub(crate) path: PathBuf,
    pub(crate) contents: Vec<u8>,
    pub(crate) render_text: bool,
}
