//! Apply an approved generation plan.

use crate::{
    plan::{self, Action, PlannedFile},
    render::RenderedOutput,
};
use std::{error::Error, fmt, fs, io::Write, path::Path};

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
            Action::Replace | Action::Skip => new.action == Action::Replace,
        };
        if !valid {
            return Err(ApplyError(format!(
                "{}: destination changed since review",
                old.path.display()
            )));
        }
    }
    for file in files {
        if file.action == Action::Skip {
            continue;
        }
        let target = root.join(&file.path);
        let parent = target
            .parent()
            .ok_or_else(|| ApplyError(format!("{}: no parent", target.display())))?;
        fs::create_dir_all(parent).map_err(|e| ApplyError(format!("{}: {e}", parent.display())))?;
        let mut options = fs::OpenOptions::new();
        options.write(true);
        match file.action {
            Action::Create => {
                options.create_new(true);
            }
            Action::Replace => {
                options.truncate(true);
            }
            Action::Skip => unreachable!(),
        }
        let mut output = options
            .open(&target)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
        output
            .write_all(&file.contents)
            .map_err(|e| ApplyError(format!("{}: {e}", target.display())))?;
    }
    Ok(())
}
