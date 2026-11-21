//! Explicit Git fetches and immutable, revision-addressed source snapshots.

use inifra_core::source_registry::{validate_git_reference, validate_git_url, validate_revision};
use std::{
    collections::BTreeMap,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub(crate) type SourceCacheError = Box<dyn Error>;
type Result<T> = std::result::Result<T, SourceCacheError>;

/// Fetches a remote ref and caches the commit it resolves to.
pub(crate) fn fetch_ref(url: &str, reference: &str, source_cache_dir: &Path) -> Result<String> {
    validate_git_url(url)?;
    validate_git_reference(reference)?;
    let remote = git(
        None,
        &["ls-remote", "--", url, reference],
        "resolve source ref",
    )?;
    let line = std::str::from_utf8(&remote.stdout)?
        .lines()
        .next()
        .ok_or("remote ref was not found")?;
    let advertised = line.split_whitespace().next().ok_or("invalid remote ref")?;
    validate_revision(advertised)?;
    let repository = temporary_repository(advertised.len())?;
    git(
        Some(repository.path()),
        &["fetch", "--depth=1", "--no-tags", "--", url, reference],
        "fetch source ref",
    )?;
    let revision = resolve_commit(repository.path(), "FETCH_HEAD")?;
    install(source_cache_dir, &revision, repository.path())?;
    Ok(revision)
}

/// Fetches and caches a previously pinned commit, reusing an existing snapshot.
pub(crate) fn fetch_revision(
    url: &str,
    reference: &str,
    revision: &str,
    source_cache_dir: &Path,
) -> Result<PathBuf> {
    validate_git_url(url)?;
    validate_git_reference(reference)?;
    validate_revision(revision)?;
    if let Some(path) = cached_revision(source_cache_dir, revision)? {
        return Ok(path);
    }

    let repository = temporary_repository(revision.len())?;
    git(
        Some(repository.path()),
        &["fetch", "--no-tags", "--", url, reference],
        "fetch source ref for pinned revision",
    )?;
    let pin_is_commit =
        resolve_commit(repository.path(), revision).is_ok_and(|resolved| resolved == revision);
    let reachable = git_command(Some(repository.path()))
        .args(["merge-base", "--is-ancestor", revision, "FETCH_HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("could not check pinned source revision: {error}"))?;
    if !pin_is_commit || !reachable.status.success() {
        return Err("pinned source revision is not reachable from the configured ref; restore that ref or update the source pin".into());
    }
    install(source_cache_dir, revision, repository.path())
}

/// Returns the existing snapshot without performing network or Git operations.
pub(crate) fn cached_revision(source_cache_dir: &Path, revision: &str) -> Result<Option<PathBuf>> {
    validate_revision(revision)?;
    match fs::symlink_metadata(source_cache_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => (),
        Ok(_) => return Err("source cache namespace must be a directory".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }

    let path = source_cache_dir.join(revision);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(Some(path)),
        Ok(_) => Err(format!("source cache entry is not a directory: {}", path.display()).into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn temporary_repository(hash_length: usize) -> Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    let mut args = vec!["init", "--bare", "--quiet"];
    if hash_length == 64 {
        args.push("--object-format=sha256");
    }
    args.push(
        directory
            .path()
            .to_str()
            .ok_or("temporary path is not UTF-8")?,
    );
    git(None, &args, "initialize temporary Git repository")?;
    Ok(directory)
}

fn resolve_commit(repository: &Path, object: &str) -> Result<String> {
    let output = git(
        Some(repository),
        &["rev-parse", "--verify", &format!("{object}^{{commit}}")],
        "resolve source commit",
    )?;
    let revision = std::str::from_utf8(&output.stdout)?.trim().to_owned();
    validate_revision(&revision)?;
    Ok(revision)
}

fn install(source_cache_dir: &Path, revision: &str, repository: &Path) -> Result<PathBuf> {
    if let Some(path) = cached_revision(source_cache_dir, revision)? {
        return Ok(path);
    }

    let parent = source_cache_dir;
    fs::create_dir_all(parent)?;
    if !fs::symlink_metadata(parent)?.file_type().is_dir() {
        return Err("source cache namespace must be a directory".into());
    }

    let temporary = tempfile::tempdir_in(parent)?;
    extract_tree(repository, revision, temporary.path())?;
    let destination = parent.join(revision);
    match fs::rename(temporary.path(), &destination) {
        Ok(()) => Ok(destination),
        Err(error) => match cached_revision(source_cache_dir, revision)? {
            Some(path) => Ok(path),
            None => Err(error.into()),
        },
    }
}

fn extract_tree(repository: &Path, revision: &str, destination: &Path) -> Result<()> {
    let output = git(
        Some(repository),
        &["ls-tree", "-r", "-z", "--full-tree", revision],
        "inspect source tree",
    )?;
    let mut portable_paths = BTreeMap::new();
    for entry in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let separator = entry
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("invalid Git tree entry")?;
        let (metadata, path) = entry.split_at(separator);
        let path = &path[1..];
        let metadata = std::str::from_utf8(metadata)?;
        let mut fields = metadata.split_whitespace();
        let mode = fields.next().ok_or("missing Git tree mode")?;
        let kind = fields.next().ok_or("missing Git tree type")?;
        let object = fields.next().ok_or("missing Git object ID")?;
        if fields.next().is_some() || kind != "blob" || !matches!(mode, "100644" | "100755") {
            return Err("source tree contains a symlink, submodule, or unsupported entry".into());
        }
        validate_revision(object)?;
        let path = std::str::from_utf8(path).map_err(|_| "source tree path is not UTF-8")?;
        validate_tree_path(path, &mut portable_paths)?;
        let target = destination.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = git(
            Some(repository),
            &["cat-file", "blob", object],
            "read source file",
        )?;
        fs::write(target, contents.stdout)?;
    }

    Ok(())
}

fn validate_tree_path(path: &str, seen: &mut BTreeMap<String, String>) -> Result<()> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err("unsafe source tree path".into());
    }

    let mut prefix = String::new();
    for part in path.split('/') {
        if !safe_component(part) {
            return Err("unsafe source tree path component".into());
        }

        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        let key = prefix.to_lowercase();
        if let Some(original) = seen.insert(key, prefix.clone())
            && original != prefix
        {
            return Err(
                "source tree contains paths that conflict on case-insensitive filesystems".into(),
            );
        }
    }

    Ok(())
}

fn safe_component(name: &str) -> bool {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "\\<>:\"|?*".contains(c))
    {
        return false;
    }

    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return false;
    }
    !stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
        .is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

fn git(repository: Option<&Path>, args: &[&str], action: &str) -> Result<Output> {
    let output = git_command(repository)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("could not {action}: {error}"))?;
    if !output.status.success() {
        let mut detail = String::from_utf8_lossy(&output.stderr).into_owned();
        for argument in args {
            if argument.contains("://") || (argument.contains('@') && argument.contains(':')) {
                detail = detail.replace(argument, "<source URL>");
            }
        }

        let detail = detail
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join("; ");
        let detail = if detail.is_empty() {
            "check the source URL, ref, credentials, and network".to_owned()
        } else {
            detail
        };

        return Err(format!(
            "could not {action} (Git exited with {}): {detail}",
            output.status
        )
        .into());
    }

    Ok(output)
}

fn git_command(repository: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(variable);
    }

    if let Some(repository) = repository {
        command.arg("--git-dir").arg(repository);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(directory)
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn file_url(path: &Path) -> String {
        let path = path.to_string_lossy().replace('\\', "/");
        let path = path
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('#', "%23")
            .replace('?', "%3F");
        if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        }
    }

    fn fixture() -> tempfile::TempDir {
        let repository = tempfile::tempdir().unwrap();
        command(
            repository.path(),
            &["init", "--quiet", "--initial-branch=main"],
        );
        fs::write(repository.path().join("README.md"), "original\n").unwrap();
        command(repository.path(), &["add", "README.md"]);
        command(repository.path(), &["commit", "--quiet", "-m", "fixture"]);
        repository
    }

    #[test]
    fn fetches_and_reuses_the_pinned_snapshot() {
        let repository = fixture();
        let cache = tempfile::tempdir().unwrap();
        let url = file_url(repository.path());
        let revision = fetch_ref(&url, "refs/heads/main", cache.path()).unwrap();
        let path = cached_revision(cache.path(), &revision).unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(path.join("README.md")).unwrap(),
            "original\n"
        );
        fs::write(repository.path().join("README.md"), "changed\n").unwrap();
        command(repository.path(), &["commit", "--quiet", "-am", "change"]);
        assert_eq!(
            fetch_revision(&url, "refs/heads/main", &revision, cache.path()).unwrap(),
            path
        );
        assert_eq!(
            fs::read_to_string(path.join("README.md")).unwrap(),
            "original\n"
        );

        let fresh_cache = tempfile::tempdir().unwrap();
        let restored =
            fetch_revision(&url, "refs/heads/main", &revision, fresh_cache.path()).unwrap();
        assert_eq!(
            fs::read_to_string(restored.join("README.md")).unwrap(),
            "original\n"
        );
    }

    #[test]
    fn rejects_pins_not_reachable_from_the_tracked_ref() {
        let repository = fixture();
        let revision = command(repository.path(), &["rev-parse", "HEAD"]);
        command(
            repository.path(),
            &["checkout", "--quiet", "--orphan", "replacement"],
        );
        fs::write(repository.path().join("README.md"), "new history\n").unwrap();
        command(repository.path(), &["add", "README.md"]);
        command(
            repository.path(),
            &["commit", "--quiet", "-m", "new history"],
        );
        let cache = tempfile::tempdir().unwrap();
        let error = fetch_revision(
            &file_url(repository.path()),
            "refs/heads/replacement",
            &revision,
            cache.path(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("not reachable"), "{error}");
        assert!(cached_revision(cache.path(), &revision).unwrap().is_none());
    }

    #[test]
    fn rejects_symbolic_links_in_git_tree() {
        let repository = fixture();
        let object = command(repository.path(), &["rev-parse", "HEAD:README.md"]);
        command(
            repository.path(),
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("120000,{object},link"),
            ],
        );
        command(repository.path(), &["commit", "--quiet", "-m", "symlink"]);
        let cache = tempfile::tempdir().unwrap();
        let url = file_url(repository.path());
        assert!(fetch_ref(&url, "refs/heads/main", cache.path()).is_err());
        assert_eq!(fs::read_dir(cache.path()).unwrap().count(), 0);
    }

    #[test]
    fn rejects_nonportable_paths_and_case_collisions() {
        for path in [
            "../escape",
            "/absolute",
            "path\\file",
            "folder/CON.txt",
            "folder/trailing.",
        ] {
            assert!(
                validate_tree_path(path, &mut BTreeMap::new()).is_err(),
                "{path}"
            );
        }

        let mut seen = BTreeMap::new();
        validate_tree_path("Folder/a", &mut seen).unwrap();
        assert!(validate_tree_path("folder/b", &mut seen).is_err());
    }
}
