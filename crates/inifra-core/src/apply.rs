//! Apply an approved generation plan.

use crate::{
    plan::{self, Action, PlannedFile},
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
    let outputs = files
        .iter()
        .map(|f| RenderedOutput {
            path: f.path.clone(),
            contents: f.contents.clone(),
        })
        .collect();
    let fresh = plan::build(root, outputs, true).map_err(|e| ApplyError(e.to_string()))?;
    for (old, new) in files.iter().zip(&fresh) {
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
    if files.iter().all(|file| file.action == Action::Skip) {
        return Ok(());
    }
    fs::create_dir_all(root).map_err(|e| ApplyError(format!("{}: {e}", root.display())))?;
    let directory = Dir::open_ambient_dir(root, ambient_authority())
        .map_err(|e| ApplyError(format!("{}: {e}", root.display())))?;
    for file in files {
        if file.action == Action::Skip {
            continue;
        }
        let target = root.join(&file.path);
        let parent = file
            .path
            .parent()
            .ok_or_else(|| ApplyError(format!("{}: no parent", target.display())))?;
        let mut parent_dir = directory
            .try_clone()
            .map_err(|e| ApplyError(e.to_string()))?;
        for component in parent.components() {
            let name = component.as_os_str();
            let next = match parent_dir.open_dir_nofollow(name) {
                Ok(dir) => dir,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match parent_dir.create_dir(name) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => {
                            return Err(ApplyError(format!("{}: {error}", parent.display())));
                        }
                    }
                    parent_dir
                        .open_dir_nofollow(name)
                        .map_err(|error| ApplyError(format!("{}: {error}", parent.display())))?
                }
                Err(error) => return Err(ApplyError(format!("{}: {error}", parent.display()))),
            };
            parent_dir = next;
        }
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
