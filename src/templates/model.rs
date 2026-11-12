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
    UserLocal,
    ProjectLocal,
    Remote,
}

impl TemplateOrigin {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Builtin => "built-in",
            Self::UserLocal => "user-local",
            Self::ProjectLocal => "project-local",
            Self::Remote => "remote",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateFile {
    pub(crate) path: PathBuf,
    pub(crate) contents: Vec<u8>,
    pub(crate) render_text: bool,
}
