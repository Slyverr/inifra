//! Apply an approved generation plan.

use crate::{
    plan::{self, Action, DirectoryAction, GenerationPlan, PlannedFile},
    render::RenderedOutput,
};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use std::{
    error::Error,
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// A plan could not be applied.
#[derive(Debug)]
pub struct ApplyError(String);
impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for ApplyError {}

/// Revalidates an approved plan and applies its non-skipped files.
///
/// # Errors
/// Returns an error if the destination changed after planning, or a filesystem operation fails.
/// Files written before an I/O error are not rolled back.
pub fn apply(root: &Path, files: &[PlannedFile]) -> Result<(), ApplyError> {
    apply_plan(
        root,
        &GenerationPlan {
            directories: Vec::new(),
            files: files.to_vec(),
        },
    )
}

/// Revalidates and applies an approved generation plan.
///
/// # Errors
/// Returns an error if the destination changed after planning, or a filesystem operation fails.
/// Files and directories created before an I/O error are not rolled back.
pub fn apply_plan(root: &Path, plan: &GenerationPlan) -> Result<(), ApplyError> {
    let outputs = plan
        .files
        .iter()
        .map(|f| RenderedOutput {
            path: f.path.clone(),
            contents: f.contents.clone(),
        })
        .collect();
    let directory_paths = plan
        .directories
        .iter()
        .map(|directory| directory.path.clone())
        .collect();
    let fresh = plan::build_with_directories(root, outputs, directory_paths, true)
        .map_err(|e| ApplyError(e.to_string()))?;
    for (old, new) in plan.files.iter().zip(&fresh.files) {
        let valid = match old.action {
            Action::Create => new.action == Action::Create,
            Action::Replace => {
                new.action == Action::Replace && old.existing_contents == new.existing_contents
            }
            Action::Skip => new.action == Action::Replace,
        };
        if !valid {
            return Err(ApplyError(format!(
                "{}: destination changed since review",
                old.path.display()
            )));
        }
    }

    for (old, new) in plan.directories.iter().zip(&fresh.directories) {
        if old.action != new.action {
            return Err(ApplyError(format!(
                "{}: destination changed since review",
                old.path.display()
            )));
        }
    }
    if plan.files.iter().all(|file| file.action == Action::Skip)
        && plan
            .directories
            .iter()
            .all(|directory| directory.action == DirectoryAction::Existing)
    {
        return Ok(());
    }
    fs::create_dir_all(root).map_err(|e| ApplyError(format!("{}: {e}", root.display())))?;
    let directory = Dir::open_ambient_dir(root, ambient_authority())
        .map_err(|e| ApplyError(format!("{}: {e}", root.display())))?;
    for planned_directory in &plan.directories {
        if planned_directory.action == DirectoryAction::Create {
            open_or_create_directory(&directory, &planned_directory.path)?;
        }
    }

    for file in &plan.files {
        if file.action == Action::Skip {
            continue;
        }

        let target = root.join(&file.path);
        let parent = file
            .path
            .parent()
            .ok_or_else(|| ApplyError(format!("{}: no parent", target.display())))?;
        let parent_dir = open_or_create_directory(&directory, parent)?;
        let filename = file
            .path
            .file_name()
            .ok_or_else(|| ApplyError(format!("{}: no file name", target.display())))?;
        match file.action {
            Action::Create => {
                let mut options = OpenOptions::new();
                options
                    .write(true)
                    .create_new(true)
                    .follow(FollowSymlinks::No);
                let mut output = parent_dir
                    .open_with(filename, &options)
                    .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
                output
                    .write_all(&file.contents)
                    .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
            }
            Action::Replace => {
                replace_file(
                    &parent_dir,
                    filename,
                    file.existing_contents.as_deref(),
                    &file.contents,
                    &target,
                )?;
            }
            Action::Skip => unreachable!(),
        }
    }
    Ok(())
}

fn open_or_create_directory(root: &Dir, path: &Path) -> Result<Dir, ApplyError> {
    let mut current = root
        .try_clone()
        .map_err(|error| ApplyError(error.to_string()))?;
    for component in path.components() {
        let name = component.as_os_str();
        let next = match current.open_dir_nofollow(name) {
            Ok(directory) => directory,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match current.create_dir(name) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(ApplyError(format!("{}: {error}", path.display()))),
                }
                current
                    .open_dir_nofollow(name)
                    .map_err(|error| ApplyError(format!("{}: {error}", path.display())))?
            }
            Err(error) => return Err(ApplyError(format!("{}: {error}", path.display()))),
        };
        current = next;
    }
    Ok(current)
}

fn replace_file(
    directory: &Dir,
    filename: &std::ffi::OsStr,
    expected_contents: Option<&[u8]>,
    contents: &[u8],
    target: &Path,
) -> Result<(), ApplyError> {
    let (temporary_name, mut temporary) = create_temporary_file(directory, target)?;
    let result = (|| {
        temporary
            .write_all(contents)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;

        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let mut existing = directory
            .open_with(filename, &options)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
        let mut current = Vec::new();
        existing
            .read_to_end(&mut current)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
        if Some(current.as_slice()) != expected_contents {
            return Err(ApplyError(format!(
                "{}: destination changed since review",
                target.display()
            )));
        }

        let permissions = existing
            .metadata()
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?
            .permissions();
        temporary
            .set_permissions(permissions)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
        temporary
            .sync_all()
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
        drop(existing);
        drop(temporary);
        directory
            .rename(&temporary_name, directory, filename)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))
    })();
    if result.is_err() {
        let _ = directory.remove_file(&temporary_name);
    }
    result
}

fn create_temporary_file(
    directory: &Dir,
    target: &Path,
) -> Result<(PathBuf, cap_std::fs::File), ApplyError> {
    for _ in 0..128 {
        let serial = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let name = PathBuf::from(format!(".inifra-{}-{serial}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        match directory.open_with(&name, &options) {
            Ok(file) => return Ok((name, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ApplyError(format!("{}: {error}", target.display()))),
        }
    }
    Err(ApplyError(format!(
        "{}: could not create a temporary replacement file",
        target.display()
    )))
}
