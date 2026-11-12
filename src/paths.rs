use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

pub(crate) fn config_root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("INIFRA_CONFIG_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            bail!("INIFRA_CONFIG_DIR must be an absolute path");
        }
        return Ok(path);
    }

    dirs::config_dir().context("could not locate the user config directory")
}

pub(crate) fn template_relative_path(label: &str, value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty()
        || value.chars().any(char::is_control)
        || value.contains('\\')
        || value.contains(':')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!(
            "template {label} path must be a safe relative path using forward slashes: {value:?}"
        );
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::template_relative_path;

    #[test]
    fn rejects_windows_specific_path_syntax_on_every_platform() {
        for path in [
            r"nested\..\outside.md",
            "C:/outside.md",
            "README.md:metadata",
        ] {
            assert!(
                template_relative_path("output", path).is_err(),
                "accepted path {path:?}"
            );
        }
    }
}
