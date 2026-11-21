use crate::{AssistantError, AssistantProfile, CredentialSource, Result, error};
use std::{collections::BTreeMap, fs, io, path::Path};
use tempfile::Builder;

/// Reads one literal credential value from Inifra's credentials file.
pub fn read_credential(path: &Path, name: &str) -> Result<Option<String>> {
    validate_name(name)?;
    let entries = read_entries(path)?;
    Ok(entries.and_then(|entries| entries.get(name).cloned()))
}

/// Resolves a profile's credential, preferring a non-empty environment value.
pub fn resolve_credential(
    profile: &AssistantProfile,
    credentials_path: &Path,
    environment: impl FnOnce(&str) -> Option<String>,
) -> Result<Option<String>> {
    profile.validate()?;
    let Some(name) = profile.api_key_env.as_deref() else {
        return Ok(None);
    };
    if let Some(value) = environment(name)
        && !value.trim().is_empty()
    {
        return Ok(Some(value));
    }
    if profile.credential_source == CredentialSource::CredentialsFile
        && let Some(value) = read_credential(credentials_path, name)?
        && !value.trim().is_empty()
    {
        return Ok(Some(value));
    }

    let source = match profile.credential_source {
        CredentialSource::Environment => format!("environment variable {name}"),
        CredentialSource::CredentialsFile => {
            format!(
                "environment variable {name} or {}",
                credentials_path.display()
            )
        }
    };
    Err(error(format!(
        "missing assistant credential; configure {source}"
    )))
}

/// Stores or replaces a literal credential without executing or shell-parsing it.
pub fn write_credential(path: &Path, name: &str, value: &str) -> Result<()> {
    validate_name(name)?;
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(error(
            "assistant credential must be non-empty and contain no control characters",
        ));
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| error("credentials path must have a parent directory"))?;
    fs::create_dir_all(parent)?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(error(format!(
            "credentials directory must be a real directory: {}",
            parent.display()
        )));
    }

    let mut entries = read_entries(path)?.unwrap_or_default();
    entries.insert(name.to_owned(), value.to_owned());
    let mut contents =
        String::from("# Inifra credentials. Values are read literally and never executed.\n");
    for (key, value) in entries {
        contents.push_str(&key);
        contents.push('=');
        contents.push_str(&value);
        contents.push('\n');
    }
    #[cfg(unix)]
    let mut temporary = {
        let mut builder = Builder::new();
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o600));
        builder.tempfile_in(parent)?
    };
    #[cfg(not(unix))]
    let mut temporary = Builder::new().tempfile_in(parent)?;
    std::io::Write::write_all(&mut temporary, contents.as_bytes())?;
    temporary
        .persist(path)
        .map_err(|persist| AssistantError(persist.error.to_string()))?;
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if crate::settings::config::valid_env_name(name) {
        Ok(())
    } else {
        Err(error(format!("invalid credential name {name:?}")))
    }
}

fn read_entries(path: &Path) -> Result<Option<BTreeMap<String, String>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(error(format!(
            "credentials path must be a regular file: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(error(format!(
                "credentials file permissions are too open: {}; run `chmod 600 {}`",
                path.display(),
                path.display()
            )));
        }
    }

    let contents = fs::read_to_string(path)?;
    let mut entries = BTreeMap::new();
    for (index, line) in contents.lines().enumerate() {
        let line = line.trim_start();
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            return Err(error(format!(
                "invalid credentials file entry on line {} in {}",
                index + 1,
                path.display()
            )));
        };
        validate_name(name).map_err(|_| {
            error(format!(
                "invalid credentials file entry on line {} in {}",
                index + 1,
                path.display()
            ))
        })?;
        if value.chars().any(char::is_control) {
            return Err(error(format!(
                "invalid credentials file entry on line {} in {}",
                index + 1,
                path.display()
            )));
        }
        if entries.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err(error(format!(
                "duplicate credential name {name:?} on line {} in {}",
                index + 1,
                path.display()
            )));
        }
    }
    Ok(Some(entries))
}
