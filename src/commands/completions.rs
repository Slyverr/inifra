use std::fs;
use std::io::Write;
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use clap_complete::Shell;
use tempfile::NamedTempFile;

use crate::theme::{self, Tone};

pub(super) fn run(shell: Shell, install: bool) -> Result<()> {
    let contents = generate_dynamic_script(shell)?;
    if install {
        install_script(shell, &contents)
    } else {
        std::io::stdout()
            .write_all(&contents)
            .context("could not write completion script")
    }
}

fn generate_dynamic_script(shell: Shell) -> Result<Vec<u8>> {
    let executable = std::env::current_exe().context("could not locate the Inifra executable")?;
    let output = ProcessCommand::new(&executable)
        .env("COMPLETE", shell.to_string())
        .output()
        .with_context(|| format!("could not generate {shell} completions"))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("could not generate {shell} completions: {error}");
    }
    if output.stdout.is_empty() {
        bail!("Inifra generated an empty {shell} completion script");
    }
    Ok(output.stdout)
}

fn install_script(shell: Shell, contents: &[u8]) -> Result<()> {
    let config_dir = crate::paths::config_root()?;
    let directory = match shell {
        Shell::Fish => config_dir.join("fish").join("completions"),
        _ => config_dir.join("inifra").join("completions"),
    };
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "could not create the {shell} completions directory {}",
            directory.display()
        )
    })?;

    let destination = directory.join(completion_filename(shell));
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
            "could not create a temporary completion file in {}",
            directory.display()
        )
    })?;
    temporary
        .write_all(contents)
        .with_context(|| format!("could not write completion file {}", destination.display()))?;
    temporary
        .persist(&destination)
        .map_err(|error| error.error)
        .with_context(|| format!("could not install completions at {}", destination.display()))?;

    println!(
        "{} {}",
        theme::paint_stdout("Installed completions at", Tone::Success, true),
        theme::paint_stdout(destination.display(), Tone::Path, false)
    );
    if shell != Shell::Fish {
        println!("Source this file from your {shell} configuration to enable completions.");
        println!("Inifra did not modify shell startup files.");
    }
    Ok(())
}

fn completion_filename(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "inifra.bash",
        Shell::Elvish => "inifra.elv",
        Shell::Fish => "inifra.fish",
        Shell::PowerShell => "inifra.ps1",
        Shell::Zsh => "_inifra",
        _ => "inifra.completion",
    }
}
