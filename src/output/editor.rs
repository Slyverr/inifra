use std::fs;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};

use super::{Plan, ensure_safe_parent};
use crate::theme::{self, Tone};

pub(crate) fn edit_text_files(plan: &mut Plan) -> Result<()> {
    let editable = plan
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.is_skipped() && std::str::from_utf8(&file.contents).is_ok())
        .map(|(index, file)| (index, file.relative_path.clone()))
        .collect::<Vec<_>>();
    if editable.is_empty() {
        println!(
            "{}",
            theme::paint_stdout("No editable text files.", Tone::Muted, false)
        );
        return Ok(());
    }

    let editor = configured_editor()?;
    let temporary = tempfile::tempdir().context("could not create a temporary edit directory")?;
    let mut paths = Vec::with_capacity(editable.len());
    for (index, relative_path) in &editable {
        let path = temporary.path().join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "could not create temporary editor directory {}",
                    parent.display()
                )
            })?;
        }
        fs::write(&path, &plan.files[*index].contents).with_context(|| {
            format!("could not prepare {} for editing", relative_path.display())
        })?;
        paths.push(path);
    }

    run_editor(editor, &paths)?;

    let edited = editable
        .iter()
        .zip(paths)
        .map(|((index, relative_path), path)| {
            ensure_safe_parent(temporary.path(), &path)?;
            let metadata = fs::symlink_metadata(&path).with_context(|| {
                format!("could not inspect edited file {}", relative_path.display())
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!(
                    "editor output must be a regular file: {}",
                    relative_path.display()
                );
            }
            fs::read(&path)
                .with_context(|| format!("could not read edited file {}", relative_path.display()))
                .map(|contents| (*index, contents))
        })
        .collect::<Result<Vec<_>>>()?;
    for (index, contents) in edited {
        plan.files[index].contents = contents;
    }
    Ok(())
}

pub(crate) fn open_files_in_editor(paths: &[PathBuf]) -> Result<()> {
    run_editor(configured_editor()?, paths)
}

fn run_editor(editor: Editor, paths: &[PathBuf]) -> Result<()> {
    let status = ProcessCommand::new(&editor.command)
        .args(editor.arguments)
        .args(paths)
        .status()
        .context("could not start the configured editor")?;
    if !status.success() {
        bail!("the configured editor exited with status {status}");
    }
    Ok(())
}

struct Editor {
    command: String,
    arguments: Vec<String>,
}

fn configured_editor() -> Result<Editor> {
    let value = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .context("set VISUAL or EDITOR to use --edit")?;
    let mut words =
        shlex::split(&value).context("could not parse the configured editor command")?;
    if words.is_empty() {
        bail!("the configured editor command is empty");
    }
    let command = words.remove(0);
    Ok(Editor {
        command,
        arguments: words,
    })
}
