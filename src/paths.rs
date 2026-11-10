use std::path::PathBuf;

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
