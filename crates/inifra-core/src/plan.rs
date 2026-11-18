//! Read-only planning of generated file operations.

use crate::{render::RenderedOutput, sources::safe_component};
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
    /// Contents observed for an existing file at planning time.
    pub existing_contents: Option<Vec<u8>>,
}

/// Whether a declared directory needs to be created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryAction {
    /// Create the directory when applying the plan.
    Create,
    /// The directory already exists and needs no changes.
    Existing,
}

/// A validated planned directory output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedDirectory {
    /// Destination relative to the target root.
    pub path: PathBuf,
    /// Action planned for this directory.
    pub action: DirectoryAction,
}

/// A complete generation plan, including files and intentionally empty directories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationPlan {
    /// Directories to create or retain.
    pub directories: Vec<PlannedDirectory>,
    /// Files to create, replace, or skip.
    pub files: Vec<PlannedFile>,
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
    Ok(build_with_directories(root, outputs, Vec::new(), overwrite)?.files)
}

/// Plans file and directory outputs without modifying the destination.
///
/// # Errors
/// Rejects unsafe, duplicate, or overlapping outputs, symlinks, and incompatible filesystem
/// entries. Existing regular files are skipped unless `overwrite` is set.
pub fn build_with_directories(
    root: &Path,
    outputs: Vec<RenderedOutput>,
    directories: Vec<PathBuf>,
    overwrite: bool,
) -> Result<GenerationPlan, PlanError> {
    validate_paths(
        outputs
            .iter()
            .map(|output| output.path.as_path())
            .chain(directories.iter().map(PathBuf::as_path)),
    )?;

    let files = outputs
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
            let existing_contents = if action == Action::Replace {
                Some(
                    fs::read(&current)
                        .map_err(|error| PlanError(format!("{}: {error}", current.display())))?,
                )
            } else {
                None
            };
            Ok(PlannedFile {
                path: output.path,
                action,
                contents: output.contents,
                existing_contents,
            })
        })
        .collect::<Result<_, _>>()?;
    let directories = directories
        .into_iter()
        .map(|path| {
            let action = inspect_directory_output(root, &path)?;
            Ok(PlannedDirectory { path, action })
        })
        .collect::<Result<_, PlanError>>()?;
    Ok(GenerationPlan { directories, files })
}

pub(crate) fn validate_paths<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
) -> Result<(), PlanError> {
    let mut seen = BTreeSet::new();
    let mut portable_paths = BTreeSet::new();
    for path in paths {
        if path.as_os_str().is_empty()
            || !path
                .components()
                .all(|c| matches!(c, Component::Normal(name) if name.to_str().is_some_and(safe_component)))
        {
            return Err(PlanError(format!(
                "unsafe output path {}",
                path.display()
            )));
        }
        if !seen.insert(path.to_path_buf()) {
            return Err(PlanError(format!("duplicate output {}", path.display())));
        }
        let portable = portable_path_key(path);
        if !portable_paths.insert(portable) {
            return Err(PlanError(format!(
                "output conflicts on case-insensitive filesystems: {}",
                path.display()
            )));
        }
    }
    for path in &seen {
        if path.ancestors().skip(1).any(|parent| seen.contains(parent)) {
            return Err(PlanError(format!("overlapping output {}", path.display())));
        }
    }
    for path in &portable_paths {
        let mut prefix = path.as_str();
        while let Some((parent, _)) = prefix.rsplit_once('/') {
            if portable_paths.contains(parent) {
                return Err(PlanError(format!(
                    "overlapping output on case-insensitive filesystems: {path}"
                )));
            }
            prefix = parent;
        }
    }
    Ok(())
}

pub(crate) fn portable_path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn inspect_directory(path: &Path) -> Result<(), PlanError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(PlanError(format!("{}: expected directory", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PlanError(format!("{}: {error}", path.display()))),
    }
}

fn inspect_directory_output(root: &Path, path: &Path) -> Result<DirectoryAction, PlanError> {
    inspect_directory(root)?;
    let mut current = root.to_path_buf();
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        current.push(component.as_os_str());
        let is_final = components.peek().is_none();
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                if is_final {
                    return Ok(DirectoryAction::Existing);
                }
            }
            Ok(_) => {
                return Err(PlanError(format!(
                    "{}: expected directory",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(DirectoryAction::Create);
            }
            Err(error) => return Err(PlanError(format!("{}: {error}", current.display()))),
        }
    }
    Err(PlanError(format!(
        "unsafe directory path {}",
        path.display()
    )))
}
