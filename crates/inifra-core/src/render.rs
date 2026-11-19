//! Render template sources without changing the destination.

use crate::sources::{Source, safe_component};
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use minijinja::{Environment, UndefinedBehavior};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs,
    io::Read,
    path::{Component, Path, PathBuf},
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

/// Checks a template source for valid UTF-8 and template syntax.
///
/// # Errors
/// Returns an error if the source cannot be read or parsed.
pub fn validate_template(path: &Path) -> Result<(), RenderError> {
    let text = fs::read_to_string(path)
        .map_err(|error| RenderError(format!("{}: {error}", path.display())))?;
    Environment::new()
        .template_from_str(&text)
        .map_err(|error| RenderError(format!("{}: {error}", path.display())))?;
    Ok(())
}

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
    render_with_references(root, sources, values, &BTreeMap::new())
}

/// Reads and renders all sources with paths to selected pack outputs.
///
/// # Errors
/// Returns an error for unreadable files, invalid template data, undefined values, or unsafe
/// source paths. No destination files are written.
pub fn render_with_references(
    root: &Path,
    sources: &[Source],
    values: &BTreeMap<String, String>,
    references: &BTreeMap<String, String>,
) -> Result<Vec<RenderedOutput>, RenderError> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    let syntax = minijinja::syntax::SyntaxConfig::builder()
        .keep_trailing_newline(true)
        .build()
        .map_err(|error| RenderError(error.to_string()))?;
    environment.set_syntax(syntax);
    let directory = Dir::open_ambient_dir(root, ambient_authority())
        .map_err(|error| RenderError(format!("{}: {error}", root.display())))?;
    let context = minijinja::context! {
        ..values,
        ..minijinja::context! { references => references }
    };
    sources
        .iter()
        .map(|source| {
            if source.source.as_os_str().is_empty() || !source.source.components().all(|component| matches!(component, Component::Normal(name) if name.to_str().is_some_and(safe_component))) {
                return Err(RenderError(format!("unsafe source path {}", source.source.display())));
            }
            let path = root.join(&source.source);
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let mut file = directory.open_with(&source.source, &options).map_err(|e| RenderError(format!("{}: {e}", path.display())))?;
            let mut data = Vec::new();
            file.read_to_end(&mut data).map_err(|e| RenderError(format!("{}: {e}", path.display())))?;
            let contents = if source.render {
                let text = std::str::from_utf8(&data)
                    .map_err(|e| RenderError(format!("{}: {e}", path.display())))?;
                environment
                    .render_str(text, &context)
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
