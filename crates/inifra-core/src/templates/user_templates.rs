//! Safe operations for template packs owned by the current user.

use crate::catalog;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Copies a read-only template pack to a new user-owned directory.
///
/// Only regular files and directories are copied. The destination must not exist.
///
/// # Errors
/// Returns an error for invalid directories, symlinks, special files, an existing destination,
/// or an unsuccessful copy. A partially copied destination is removed when copying fails.
pub fn copy_pack(source: &Path, destination: &Path) -> io::Result<()> {
    ensure_directory(source, "template source")?;
    ensure_missing(destination, "template destination")?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    ensure_directory(parent, "template destination parent")?;

    let result = copy_directory(source, destination);
    if let Err(error) = result {
        if let Err(cleanup_error) = fs::remove_dir_all(destination) {
            return Err(io::Error::other(format!(
                "{error}; removing partial template copy also failed: {cleanup_error}"
            )));
        }
        return Err(error);
    }
    Ok(())
}

/// Creates a user-owned template pack with a valid manifest and one empty source file.
///
/// The category directory is created when it does not exist. The returned path identifies the
/// new pack so the caller can open its manifest in an editor.
///
/// # Errors
/// Returns an error for invalid IDs, descriptions, or source paths, unsafe directories, or an
/// existing pack.
pub fn create_pack(
    templates_root: &Path,
    category: &str,
    id: &str,
    description: &str,
    first_source: &Path,
) -> io::Result<PathBuf> {
    validate_identifier(category, "category")?;
    validate_identifier(id, "template")?;
    if description.trim().is_empty() || description.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "template description must be nonempty and contain no control characters",
        ));
    }
    validate_source_path(first_source)?;

    if let Ok(metadata) = fs::symlink_metadata(templates_root)
        && !metadata.is_dir()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "template catalog must be a directory: {}",
                templates_root.display()
            ),
        ));
    }
    fs::create_dir_all(templates_root)?;
    let category_path = templates_root.join(category);
    if let Ok(metadata) = fs::symlink_metadata(&category_path)
        && (!metadata.is_dir() || metadata.file_type().is_symlink())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "template category must be a regular directory: {}",
                category_path.display()
            ),
        ));
    }
    fs::create_dir_all(&category_path)?;

    let pack = category_path.join(id);
    ensure_missing(&pack, "template pack")?;
    fs::create_dir(&pack)?;
    let description = toml::Value::String(description.to_owned()).to_string();
    let manifest = format!("schema_version = 2\ndescription = {description}\n");
    let source_path = pack.join(first_source);
    let result = (|| {
        if let Some(parent) = source_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(source_path)?;
        fs::write(pack.join("template.toml"), manifest)
    })();
    if let Err(error) = result {
        if let Err(cleanup_error) = fs::remove_dir_all(&pack) {
            return Err(io::Error::other(format!(
                "{error}; removing incomplete template also failed: {cleanup_error}"
            )));
        }
        return Err(error);
    }
    Ok(pack)
}

fn validate_source_path(path: &Path) -> io::Result<()> {
    let input = path.to_string_lossy();
    if input.ends_with(['/', '\\']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "first source file must name a file, not a directory",
        ));
    }
    if path == Path::new("template.toml") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "template.toml is reserved for the pack manifest",
        ));
    }

    let output_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".j2").or(Some(name)))
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "first source file must have a valid output name",
            )
        })?;
    let output_path = path.with_file_name(output_name);
    let result = if path == output_path {
        crate::plan::validate_paths([path])
    } else {
        crate::plan::validate_paths([path, output_path.as_path()])
    };
    result.map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))
}

/// Checks whether a template ID follows the catalog's lowercase kebab-case contract.
///
/// # Errors
/// Returns an error when the identifier is invalid.
pub fn validate_id(id: &str) -> io::Result<()> {
    validate_identifier(id, "template")
}

/// Renames a user-owned template pack within its category.
///
/// # Errors
/// Returns an error when either ID is invalid, the source is missing or unsafe, the destination
/// already exists, or the filesystem rename fails.
pub fn rename_pack(
    templates_root: &Path,
    category: &str,
    old_id: &str,
    new_id: &str,
) -> io::Result<()> {
    validate_identifier(category, "category")?;
    validate_identifier(old_id, "template")?;
    validate_identifier(new_id, "template")?;
    if old_id == new_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "new template ID is the same as the current ID",
        ));
    }

    let category_path = templates_root.join(category);
    ensure_directory(&category_path, "template category")?;
    let source = category_path.join(old_id);
    ensure_pack_directory(&source)?;
    let destination = category_path.join(new_id);
    ensure_missing(&destination, "template destination")?;
    fs::rename(source, destination)
}

/// Removes a user-owned template pack and its contents.
///
/// # Errors
/// Returns an error when the pack is missing, contains a symlink or special file, or cannot be
/// removed.
pub fn remove_pack(templates_root: &Path, category: &str, id: &str) -> io::Result<()> {
    validate_identifier(category, "category")?;
    validate_identifier(id, "template")?;
    let category_path = templates_root.join(category);
    ensure_directory(&category_path, "template category")?;
    let pack = category_path.join(id);
    ensure_pack_directory(&pack)?;
    fs::remove_dir_all(pack)
}

/// Renames references from one template ID to another in user-owned pack manifests.
///
/// # Errors
/// Returns an error for malformed manifests, unsafe entries, or failed writes. If a write fails,
/// earlier manifest changes are restored when possible.
pub fn rename_references(templates_root: &Path, old_id: &str, new_id: &str) -> io::Result<usize> {
    validate_identifier(old_id, "template")?;
    validate_identifier(new_id, "template")?;
    let categories = match fs::read_dir(templates_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };

    let mut manifests = Vec::new();
    for category in categories {
        let category = category?;
        let kind = category.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "template catalog cannot contain symlinks: {}",
                    category.path().display()
                ),
            ));
        }
        if !kind.is_dir() {
            continue;
        }

        for pack in fs::read_dir(category.path())? {
            let pack = pack?;
            let kind = pack.file_type()?;
            if kind.is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "template catalog cannot contain symlinks: {}",
                        pack.path().display()
                    ),
                ));
            }
            if kind.is_dir() {
                manifests.push(pack.path().join("template.toml"));
            }
        }
    }
    manifests.sort();

    let mut updates = Vec::new();
    for path in manifests {
        let original = crate::safe_fs::read_regular_file(&path, "template manifest")?;
        let mut manifest: toml::Value = toml::from_str(&original).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {error}", path.display()),
            )
        })?;
        let mut changed = false;
        if let Some(references) = manifest
            .get_mut("references")
            .and_then(toml::Value::as_table_mut)
        {
            for reference in references
                .iter_mut()
                .filter_map(|(_, value)| value.as_table_mut())
            {
                if reference.get("template").and_then(toml::Value::as_str) == Some(old_id) {
                    reference.insert(
                        "template".to_owned(),
                        toml::Value::String(new_id.to_owned()),
                    );
                    changed = true;
                }
            }
        }
        if changed {
            let replacement = toml::to_string_pretty(&manifest).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {error}", path.display()),
                )
            })?;
            updates.push((path, original, replacement));
        }
    }

    let mut written = Vec::new();
    for (path, original, replacement) in &updates {
        if let Err(error) = crate::safe_fs::write_atomic(path, replacement.as_bytes()) {
            let rollback_errors = written
                .iter()
                .filter_map(|(path, original): &(PathBuf, String)| {
                    crate::safe_fs::write_atomic(path, original.as_bytes())
                        .err()
                        .map(|error| format!("{}: {error}", path.display()))
                })
                .collect::<Vec<_>>();
            let message = if rollback_errors.is_empty() {
                format!("{}: {error}", path.display())
            } else {
                format!(
                    "{}: {error}; restoring earlier template references also failed: {}",
                    path.display(),
                    rollback_errors.join("; ")
                )
            };
            return Err(io::Error::other(message));
        }
        written.push((path.clone(), original.clone()));
    }
    Ok(updates.len())
}

fn validate_identifier(value: &str, kind: &str) -> io::Result<()> {
    if catalog::valid_identifier(value) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid {kind} ID {value:?}; use lowercase letters, digits, and hyphens"),
        ))
    }
}

fn ensure_missing(path: &Path, description: &str) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{description} already exists: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn ensure_directory(path: &Path, description: &str) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{description} must be a regular directory: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn ensure_pack_directory(path: &Path) -> io::Result<()> {
    ensure_directory(path, "template pack")?;
    validate_tree(path)
}

fn validate_tree(directory: &Path) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("template packs cannot contain symlinks: {}", path.display()),
            ));
        }
        if kind.is_dir() {
            validate_tree(&path)?;
        } else if !kind.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "template packs cannot contain special files: {}",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "template packs cannot contain symlinks: {}",
                    source_path.display()
                ),
            ));
        }
        if kind.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else if kind.is_file() {
            fs::copy(source_path, destination_path)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "template packs cannot contain special files: {}",
                    source_path.display()
                ),
            ));
        }
    }
    Ok(())
}
