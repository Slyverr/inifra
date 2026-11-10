use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct Template {
    pub(crate) id: String,
    pub(crate) description: String,
    pub(crate) files: Vec<TemplateFile>,
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateFile {
    pub(crate) path: PathBuf,
    pub(crate) contents: Vec<u8>,
}
