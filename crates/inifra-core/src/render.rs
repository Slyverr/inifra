//! Render template sources without changing the destination.

use crate::sources::Source;
use minijinja::{Environment, UndefinedBehavior};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

/// An output prepared in memory.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RenderedOutput {
    /// Destination relative to the generation root.
    pub path: PathBuf,
    /// Rendered text or unchanged static bytes.
    pub contents: Vec<u8>,
}

/// A source could not be read or rendered.
#[derive(Debug)]
pub struct RenderError(String);
impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for RenderError {}

/// Reads and renders all sources using the supplied input values.
///
/// # Errors
/// Returns an error for unreadable files, invalid UTF-8 in templates, invalid template syntax,
/// or undefined values. No destination files are written.
pub fn render(
    root: &Path,
    sources: &[Source],
    values: &BTreeMap<String, String>,
) -> Result<Vec<RenderedOutput>, RenderError> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    let syntax = minijinja::syntax::SyntaxConfig::builder()
        .keep_trailing_newline(true)
        .build()
        .map_err(|error| RenderError(error.to_string()))?;
    environment.set_syntax(syntax);
    sources
        .iter()
        .map(|source| {
            let path = root.join(&source.source);
            let data =
                fs::read(&path).map_err(|e| RenderError(format!("{}: {e}", path.display())))?;
            let contents = if source.render {
                let text = std::str::from_utf8(&data)
                    .map_err(|e| RenderError(format!("{}: {e}", path.display())))?;
                environment
                    .render_str(text, values)
                    .map_err(|e| RenderError(format!("{}: {e}", path.display())))?
                    .into_bytes()
            } else {
                data
            };
            Ok(RenderedOutput {
                path: source.output.clone(),
                contents,
            })
        })
        .collect()
}
