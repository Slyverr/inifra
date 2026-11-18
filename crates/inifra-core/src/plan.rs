//! Read-only planning of generated file operations.

use crate::render::RenderedOutput;
use std::{
    collections::BTreeSet,
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

/// The action to take for an output file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    /// Create a new file.
    Create,
    /// Replace an existing regular file.
    Replace,
    /// Leave an existing regular file unchanged.
    Skip,
}

/// A validated planned output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedFile {
    /// Destination relative to the target root.
    pub path: PathBuf,
    /// Action planned for this file.
    pub action: Action,
    /// Bytes to write when creating or replacing the file.
    pub contents: Vec<u8>,
}

/// Invalid output or destination state.
#[derive(Debug)]
pub struct PlanError(String);
impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for PlanError {}

/// Plans writes without modifying the destination.
///
/// # Errors
/// Rejects unsafe paths, duplicate or overlapping outputs, symlinks, and incompatible
/// filesystem entries. Existing regular files are skipped unless `overwrite` is set.
pub fn build(
    root: &Path,
    outputs: Vec<RenderedOutput>,
    overwrite: bool,
) -> Result<Vec<PlannedFile>, PlanError> {
    let mut seen = BTreeSet::new();
    for output in &outputs {
        if output.path.as_os_str().is_empty()
            || !output
                .path
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(PlanError(format!(
                "unsafe output path {}",
                output.path.display()
            )));
        }
        if !seen.insert(output.path.clone()) {
            return Err(PlanError(format!(
                "duplicate output {}",
                output.path.display()
            )));
        }
    }
    for path in &seen {
        if path.ancestors().skip(1).any(|parent| seen.contains(parent)) {
            return Err(PlanError(format!("overlapping output {}", path.display())));
        }
    }
    outputs
        .into_iter()
        .map(|output| {
            let mut current = root.to_path_buf();
            inspect_directory(&current)?;
            let mut components = output.path.components().peekable();
            while let Some(component) = components.next() {
                current.push(component.as_os_str());
                if components.peek().is_some() {
                    inspect_directory(&current)?;
                }
            }
            let action = match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    if overwrite {
                        Action::Replace
                    } else {
                        Action::Skip
                    }
                }
                Ok(_) => {
                    return Err(PlanError(format!(
                        "{}: output is not a regular file",
                        current.display()
                    )));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Action::Create,
                Err(error) => return Err(PlanError(format!("{}: {error}", current.display()))),
            };
            Ok(PlannedFile {
                path: output.path,
                action,
                contents: output.contents,
            })
        })
        .collect()
}

fn inspect_directory(path: &Path) -> Result<(), PlanError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(PlanError(format!("{}: expected directory", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PlanError(format!("{}: {error}", path.display()))),
    }
}
