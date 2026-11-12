use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::registry::{self, ProjectSource};

pub(super) fn fetch_and_materialize(
    git_directory: &Path,
    url: &str,
    revision: &str,
    directory: &str,
    destination: &Path,
) -> Result<String> {
    registry::validate_url(url)?;
    registry::validate_directory(directory)?;
    validate_fetch_reference(revision)?;

    run_git(None, ["init", "--bare", "--quiet"], Some(git_directory))
        .context("could not initialize Git source cache")?;
    run_git(
        Some(git_directory),
        ["fetch", "--quiet", "--no-tags", "--depth=1", url, revision],
        None,
    )
    .with_context(|| format!("could not fetch source at {revision:?}"))?;
    let resolved_revision = git_output(
        Some(git_directory),
        ["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
    )?;
    registry::validate_revision(&resolved_revision)?;

    let source = ProjectSource {
        url: url.to_owned(),
        revision: resolved_revision.clone(),
        directory: directory.to_owned(),
    };
    let tree = git_output_bytes(
        Some(git_directory),
        [
            "ls-tree",
            "-r",
            "-z",
            "--full-tree",
            &source.revision,
            "--",
            &source.directory,
        ],
    )?;
    materialize_tree(git_directory, &source, &tree, destination)?;
    Ok(resolved_revision)
}

fn materialize_tree(
    git_directory: &Path,
    source: &ProjectSource,
    tree: &[u8],
    destination: &Path,
) -> Result<()> {
    fs::create_dir(destination).with_context(|| {
        format!(
            "could not create temporary source tree {}",
            destination.display()
        )
    })?;
    let prefix = format!("{}/", source.directory.trim_end_matches('/'));
    for record in tree
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = std::str::from_utf8(record).context("Git source contains a non-UTF-8 path")?;
        let (metadata, path) = record
            .split_once('\t')
            .context("Git returned a malformed tree entry")?;
        let mut fields = metadata.split_whitespace();
        let mode = fields.next().context("Git tree entry has no mode")?;
        let kind = fields.next().context("Git tree entry has no type")?;
        let object = fields.next().context("Git tree entry has no object ID")?;
        if fields.next().is_some() {
            bail!("Git returned a malformed tree entry");
        }
        if mode != "100644" && mode != "100755" || kind != "blob" {
            bail!("remote template source contains a symlink or non-file entry: {path}");
        }
        let relative = path
            .strip_prefix(&prefix)
            .with_context(|| format!("Git returned a path outside source directory: {path}"))?;
        let relative = validate_tree_path(relative)?;
        let target = destination.join(relative);
        let parent = target
            .parent()
            .context("remote template file has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create source directory {}", parent.display()))?;
        let contents = git_output_bytes(Some(git_directory), ["cat-file", "blob", object])?;
        fs::write(&target, contents)
            .with_context(|| format!("could not materialize source file {}", target.display()))?;
    }
    Ok(())
}

fn validate_tree_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.to_string_lossy().contains('\\')
        || path.to_string_lossy().contains(':')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!(
            "remote template source contains an unsafe path: {}",
            path.display()
        );
    }
    Ok(path.to_path_buf())
}

pub(super) fn validate_fetch_reference(reference: &str) -> Result<()> {
    if reference.trim().is_empty()
        || reference.starts_with('-')
        || reference
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        bail!("source revision must be a Git reference without whitespace or control characters");
    }
    Ok(())
}

fn run_git<const N: usize>(
    git_directory: Option<&Path>,
    args: [&str; N],
    init_path: Option<&Path>,
) -> Result<()> {
    let mut command = Command::new("git");
    if let Some(directory) = git_directory {
        command.arg("--git-dir").arg(directory);
    }
    command.args(args);
    if let Some(path) = init_path {
        command.arg(path);
    }
    command
        .env("GIT_ALLOW_PROTOCOL", "https:ssh:git:file")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = command
        .output()
        .context("could not start Git; install Git to use project sources")?;
    if !output.status.success() {
        bail!(
            "Git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn git_output<const N: usize>(git_directory: Option<&Path>, args: [&str; N]) -> Result<String> {
    let bytes = git_output_bytes(git_directory, args)?;
    String::from_utf8(bytes)
        .context("Git returned output that was not valid UTF-8")
        .map(|value| value.trim().to_owned())
}

fn git_output_bytes<const N: usize>(
    git_directory: Option<&Path>,
    args: [&str; N],
) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    if let Some(directory) = git_directory {
        command.arg("--git-dir").arg(directory);
    }
    command
        .args(args)
        .env("GIT_ALLOW_PROTOCOL", "https:ssh:git:file")
        .stdin(Stdio::null())
        .output()
        .context("could not start Git; install Git to use project sources")
        .and_then(|output| {
            if output.status.success() {
                Ok(output.stdout)
            } else {
                bail!(
                    "Git failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )
            }
        })
}
