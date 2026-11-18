//! Apply an approved generation plan.

use crate::{
    plan::{self, Action, PlannedFile},
    render::RenderedOutput,
};
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use std::{
    error::Error,
    fmt, fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

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
        if !parent.as_os_str().is_empty() {
            directory
                .create_dir_all(parent)
                .map_err(|e| ApplyError(format!("{}: {e}", parent.display())))?;
        }
        let mut options = OpenOptions::new();
        options.write(true);
        options.follow(FollowSymlinks::No);
        match file.action {
            Action::Create => {
                options.create_new(true);
            }
            Action::Replace => {
                options.read(true);
            }
            Action::Skip => unreachable!(),
        }
        let mut output = directory
            .open_with(&file.path, &options)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
        if file.action == Action::Replace {
            let mut current = Vec::new();
            output
                .read_to_end(&mut current)
                .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
            if Some(&current) != file.existing_contents.as_ref() {
                return Err(ApplyError(format!(
                    "{}: destination changed since review",
                    file.path.display()
                )));
            }
            output
                .set_len(0)
                .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
            output
                .seek(SeekFrom::Start(0))
                .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
        }
        output
            .write_all(&file.contents)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
    }
    Ok(())
}
