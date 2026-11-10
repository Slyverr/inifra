use std::fs;
use std::io::Write;

use anyhow::{Context, Result, bail};
use clap_complete::{Shell, generate};
use tempfile::NamedTempFile;

use crate::cli;

const FISH_COMPLETION_FILE: &str = "inifra.generated.fish";

pub(super) fn run(shell: Shell, apply: bool) -> Result<()> {
    if apply && shell != Shell::Fish {
        bail!("--apply is currently supported only for Fish");
    }

    let mut command = cli::command();
    let mut contents = Vec::new();
    generate(shell, &mut command, "inifra", &mut contents);

    if apply {
        install_fish_completions(&contents)
    } else {
        std::io::stdout()
            .write_all(&contents)
            .context("could not write completion script")
    }
}

fn install_fish_completions(contents: &[u8]) -> Result<()> {
    let config_dir = dirs::config_dir().context("could not locate the user config directory")?;
    let directory = config_dir.join("fish").join("completions");
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "could not create Fish completions directory {}",
            directory.display()
        )
    })?;

    let destination = directory.join(FISH_COMPLETION_FILE);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!(
                    "completion destination must be a regular file: {}",
                    destination.display()
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("could not inspect {}", destination.display()));
        }
    }

    let mut temporary = NamedTempFile::new_in(&directory).with_context(|| {
        format!(
            "could not create a temporary file in {}",
            directory.display()
        )
    })?;
    temporary.write_all(contents).with_context(|| {
        format!(
            "could not write Fish completions in {}",
            directory.display()
        )
    })?;
    temporary
        .persist(&destination)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "could not install Fish completions at {}",
                destination.display()
            )
        })?;
    println!("Installed Fish completions at {}", destination.display());
    Ok(())
}
