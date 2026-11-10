use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use tempfile::Builder;

use crate::templates::TemplateFile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Create,
    Skip,
    Replace,
}

struct PlannedFile {
    relative_path: PathBuf,
    target_path: PathBuf,
    contents: Vec<u8>,
    action: Action,
}

pub(crate) struct Plan {
    destination: PathBuf,
    files: Vec<PlannedFile>,
}

#[derive(Default)]
pub(crate) struct WriteSummary {
    pub(crate) created: usize,
    pub(crate) replaced: usize,
    pub(crate) skipped: usize,
}

impl PlannedFile {
    fn is_skipped(&self) -> bool {
        self.action == Action::Skip
    }
}

impl Plan {
    pub(crate) fn all_skipped(&self) -> bool {
        self.files.iter().all(PlannedFile::is_skipped)
    }
}

pub(crate) fn edit_text_files(plan: &mut Plan) -> Result<()> {
    let editable = plan
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.is_skipped() && std::str::from_utf8(&file.contents).is_ok())
        .map(|(index, file)| (index, file.relative_path.clone()))
        .collect::<Vec<_>>();
    if editable.is_empty() {
        println!("No editable text files.");
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

pub(crate) fn plan(
    destination: &Path,
    selection: &str,
    outputs: &[TemplateFile],
    overwrite: bool,
) -> Result<Plan> {
    let destination = resolve_destination(destination)?;
    let mut files = Vec::with_capacity(outputs.len());
    let mut output_paths = Vec::with_capacity(outputs.len());

    for output in outputs {
        let relative_path = validate_relative_path(&output.path)?;
        if output_paths.iter().any(|existing: &PathBuf| {
            existing.starts_with(&relative_path) || relative_path.starts_with(existing)
        }) {
            bail!(
                "template {:?} declares conflicting output paths near {}",
                selection,
                output.path.display()
            );
        }
        output_paths.push(relative_path.clone());

        let target_path = destination.join(&relative_path);
        ensure_safe_parent(&destination, &target_path)?;
        let action = match inspect_target(&target_path)? {
            Target::Missing => Action::Create,
            Target::File if overwrite => Action::Replace,
            Target::File => Action::Skip,
        };
        files.push(PlannedFile {
            relative_path,
            target_path,
            contents: output.contents.clone(),
            action,
        });
    }

    Ok(Plan { destination, files })
}

fn resolve_destination(destination: &Path) -> Result<PathBuf> {
    let absolute = if destination.is_absolute() {
        destination.to_path_buf()
    } else {
        std::env::current_dir()
            .context("could not determine the current directory")?
            .join(destination)
    };

    match fs::metadata(&absolute) {
        Ok(metadata) if metadata.is_dir() => fs::canonicalize(&absolute)
            .with_context(|| format!("could not resolve destination {}", absolute.display())),
        Ok(_) => bail!("destination is not a directory: {}", absolute.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(absolute),
        Err(error) => Err(error)
            .with_context(|| format!("could not inspect destination {}", absolute.display())),
    }
}

fn validate_relative_path(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty()
        || path.to_string_lossy().contains('\0')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("refusing unsafe template output path: {}", path.display());
    }
    Ok(path.to_path_buf())
}

fn ensure_safe_parent(root: &Path, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .context("template output path has no parent")?;
    let relative = parent.strip_prefix(root).with_context(|| {
        format!(
            "output path {} escaped destination {}",
            target.display(),
            root.display()
        )
    })?;

    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("refusing unsafe output directory: {}", parent.display());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "refusing to write through symlink directory: {}",
                    current.display()
                );
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!("output parent is not a directory: {}", current.display());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not inspect output directory {}", current.display())
                });
            }
        }
    }
    Ok(())
}

enum Target {
    Missing,
    File,
}

fn inspect_target(path: &Path) -> Result<Target> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("refusing to write through symlink: {}", path.display());
        }
        Ok(metadata) if metadata.is_file() => Ok(Target::File),
        Ok(metadata) if metadata.is_dir() => {
            bail!("output path is a directory: {}", path.display())
        }
        Ok(_) => bail!("output path is not a regular file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Target::Missing),
        Err(error) => Err(error).with_context(|| format!("could not inspect {}", path.display())),
    }
}

pub(crate) fn show_plan(plan: &Plan, dry_run: bool) {
    let heading = if dry_run { "Preview for" } else { "Target:" };
    println!("{heading} {}", plan.destination.display());
    for file in &plan.files {
        let action = match file.action {
            Action::Create => "create",
            Action::Skip => "skip (already exists)",
            Action::Replace => "replace",
        };
        println!("  {action:<22} {}", file.relative_path.display());
    }
}

pub(crate) fn show_content_preview(outputs: &[TemplateFile]) {
    const MAX_PREVIEW_LINES: usize = 40;

    println!("\nSelected file preview:");
    for output in outputs {
        println!("\n--- {} ---", output.path.display());
        match std::str::from_utf8(&output.contents) {
            Ok(contents) => {
                let mut lines = contents.lines();
                for line in lines.by_ref().take(MAX_PREVIEW_LINES) {
                    let safe_line = line
                        .chars()
                        .map(|character| {
                            if character.is_control() {
                                format!("\\u{{{:x}}}", u32::from(character))
                            } else {
                                character.to_string()
                            }
                        })
                        .collect::<String>();
                    println!("{safe_line}");
                }
                if lines.next().is_some() {
                    println!("… preview truncated");
                }
            }
            Err(_) => println!("(binary content; preview omitted)"),
        }
    }
}

pub(crate) fn write_plan(plan: &Plan) -> Result<WriteSummary> {
    fs::create_dir_all(&plan.destination).with_context(|| {
        format!(
            "could not create destination {}",
            plan.destination.display()
        )
    })?;

    let mut summary = WriteSummary::default();
    for file in &plan.files {
        if file.action == Action::Skip {
            summary.skipped += 1;
            continue;
        }

        ensure_safe_parent(&plan.destination, &file.target_path)?;
        let parent = file
            .target_path
            .parent()
            .context("template output path has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create output directory {}", parent.display()))?;
        ensure_safe_parent(&plan.destination, &file.target_path)?;

        match (file.action, inspect_target(&file.target_path)?) {
            (Action::Create, Target::Missing) | (Action::Replace, Target::File) => {}
            (Action::Create, Target::File) => {
                bail!(
                    "output appeared after planning; refusing to replace {}",
                    file.target_path.display()
                );
            }
            (Action::Replace, Target::Missing) => {
                bail!(
                    "output disappeared after planning; refusing to create {}",
                    file.target_path.display()
                );
            }
            (Action::Skip, _) => continue,
        }

        let mut builder = Builder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o666));
        }
        let mut temporary = builder.tempfile_in(parent).with_context(|| {
            format!(
                "could not prepare output for {}",
                file.target_path.display()
            )
        })?;
        temporary.write_all(&file.contents).with_context(|| {
            format!("could not write output for {}", file.target_path.display())
        })?;

        match file.action {
            Action::Create => {
                temporary
                    .persist_noclobber(&file.target_path)
                    .map_err(|error| error.error)
                    .with_context(|| format!("could not create {}", file.target_path.display()))?;
                summary.created += 1;
            }
            Action::Replace => {
                temporary
                    .persist(&file.target_path)
                    .map_err(|error| error.error)
                    .with_context(|| format!("could not replace {}", file.target_path.display()))?;
                summary.replaced += 1;
            }
            Action::Skip => continue,
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::TemplateFile;

    fn files(files: &[(&str, &str)]) -> Vec<TemplateFile> {
        files
            .iter()
            .map(|(path, contents)| TemplateFile {
                path: PathBuf::from(path),
                contents: contents.as_bytes().to_vec(),
                render_text: false,
            })
            .collect()
    }
    #[test]
    fn rejects_empty_absolute_and_traversing_paths() {
        for path in ["", ".", "../outside", "nested/../../outside", "/absolute"] {
            assert!(
                validate_relative_path(Path::new(path)).is_err(),
                "accepted unsafe path {path:?}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_drive_paths() {
        assert!(validate_relative_path(Path::new(r"C:\outside")).is_err());
    }

    #[test]
    fn rejects_duplicate_output_paths() {
        let destination = tempfile::tempdir().expect("create destination");
        let files = files(&[("README.md", "first"), ("README.md", "second")]);

        assert!(plan(destination.path(), "test", &files, false).is_err());
    }

    #[test]
    fn rejects_file_and_directory_output_collisions_before_writing() {
        let destination = tempfile::tempdir().expect("create destination");
        let files = files(&[("created.md", "first"), ("created.md/nested.txt", "second")]);

        assert!(plan(destination.path(), "test", &files, false).is_err());
        assert!(!destination.path().join("created.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_output_files() {
        use std::os::unix::fs::symlink;

        let destination = tempfile::tempdir().expect("create destination");
        let outside = tempfile::NamedTempFile::new().expect("create outside file");
        symlink(outside.path(), destination.path().join("README.md")).expect("create symlink");
        let files = files(&[("README.md", "data")]);

        assert!(plan(destination.path(), "test", &files, true).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_output_directories() {
        use std::os::unix::fs::symlink;

        let destination = tempfile::tempdir().expect("create destination");
        let outside = tempfile::tempdir().expect("create outside directory");
        symlink(outside.path(), destination.path().join("docs")).expect("create symlink");
        let files = files(&[("docs/README.md", "data")]);

        assert!(plan(destination.path(), "test", &files, false).is_err());
        assert!(!outside.path().join("README.md").exists());
    }
}
