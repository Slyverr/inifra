use clap_complete::{Shell, env::Shells};
use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

use crate::completions::COMPLETION_ENV;

pub(super) const COMPLETION_MARKER: &str = "# Inifra generated completion file";

pub(crate) fn run(
    shell: Shell,
    install: bool,
    uninstall: bool,
    overwrite: bool,
    force: bool,
) -> Result<(), Box<dyn Error>> {
    validate_shell(shell)?;
    let path = if install || uninstall {
        Some(install_path(shell)?)
    } else {
        None
    };

    if uninstall {
        let path = path.as_ref().expect("an install path was resolved");
        if uninstall_script(path, force)? {
            println!(
                "{} {}",
                inifra_tui::presentation::paint(
                    "Removed completions:",
                    inifra_tui::presentation::Tone::Success,
                    false
                ),
                inifra_tui::presentation::paint(
                    inifra_tui::presentation::terminal_text(&path.display().to_string()),
                    inifra_tui::presentation::Tone::Path,
                    false
                )
            );
            if let Some(instruction) = deactivation_instruction(shell, path) {
                println!(
                    "{} {}",
                    inifra_tui::presentation::paint(
                        "Also remove:",
                        inifra_tui::presentation::Tone::Muted,
                        false
                    ),
                    inifra_tui::presentation::paint(
                        inifra_tui::presentation::terminal_text(&instruction),
                        inifra_tui::presentation::Tone::Accent,
                        false
                    )
                );
            }
        } else {
            println!(
                "{}",
                inifra_tui::presentation::paint(
                    "No Inifra completion file was installed.",
                    inifra_tui::presentation::Tone::Muted,
                    false
                )
            );
        }

        return Ok(());
    }

    let script = registration_script(shell)?;
    if install {
        let path = path.as_ref().expect("an install path was resolved");
        let changed = write_script(path, script.as_bytes(), overwrite)?;
        let action = if changed {
            "Wrote completion file:"
        } else {
            "Completion file is up to date:"
        };
        println!(
            "{} {}",
            inifra_tui::presentation::paint(action, inifra_tui::presentation::Tone::Success, false),
            inifra_tui::presentation::paint(
                inifra_tui::presentation::terminal_text(&path.display().to_string()),
                inifra_tui::presentation::Tone::Path,
                false
            )
        );
        if let Some(instruction) = activation_instruction(shell, path) {
            println!(
                "{} {}",
                inifra_tui::presentation::paint(
                    "To activate:",
                    inifra_tui::presentation::Tone::Muted,
                    false
                ),
                inifra_tui::presentation::paint(
                    inifra_tui::presentation::terminal_text(&instruction),
                    inifra_tui::presentation::Tone::Accent,
                    false
                )
            );
        }
    } else {
        io::stdout().write_all(script.as_bytes())?;
    }

    Ok(())
}

fn validate_shell(shell: Shell) -> Result<(), Box<dyn Error>> {
    if matches!(
        shell,
        Shell::Bash | Shell::Elvish | Shell::Fish | Shell::PowerShell | Shell::Zsh
    ) {
        Ok(())
    } else {
        Err(format!("unsupported completion shell {shell}").into())
    }
}

pub(super) fn registration_script(shell: Shell) -> Result<String, Box<dyn Error>> {
    let name = shell.to_string();
    let shells = Shells::builtins();
    let completer = shells
        .completer(&name)
        .ok_or_else(|| format!("no completion generator is available for {name}"))?;
    let mut script = Vec::new();
    completer.write_registration(COMPLETION_ENV, "inifra", "inifra", "inifra", &mut script)?;
    let script = String::from_utf8(script)?;
    Ok(format!("{COMPLETION_MARKER}\n{script}"))
}

pub(super) fn install_path(shell: Shell) -> Result<PathBuf, Box<dyn Error>> {
    let base = directories::BaseDirs::new().ok_or("could not determine user directories")?;
    let config = user_root(
        shell,
        std::env::var_os("XDG_CONFIG_HOME"),
        base.home_dir(),
        base.config_dir(),
        Path::new(".config"),
        cfg!(windows),
    )?;
    let data = user_root(
        shell,
        std::env::var_os("XDG_DATA_HOME"),
        base.home_dir(),
        base.data_local_dir(),
        Path::new(".local/share"),
        cfg!(windows),
    )?;
    shell_completion_path(shell, &config, &data)
}

pub(super) fn user_root(
    shell: Shell,
    override_path: Option<std::ffi::OsString>,
    home: &Path,
    platform_path: &Path,
    unix_fallback: &Path,
    windows: bool,
) -> io::Result<PathBuf> {
    if let Some(override_path) = override_path.filter(|path| !path.is_empty()) {
        let path = PathBuf::from(override_path);
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "XDG user directories must be absolute paths",
            ));
        }

        return Ok(path);
    }

    Ok(if windows && shell == Shell::PowerShell {
        platform_path.to_path_buf()
    } else {
        home.join(unix_fallback)
    })
}

pub(super) fn shell_completion_path(
    shell: Shell,
    config: &Path,
    data: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    Ok(match shell {
        Shell::Bash => data.join("bash-completion/completions/inifra.bash"),
        Shell::Elvish => config.join("elvish/lib/inifra.elv"),
        Shell::Fish => config.join("fish/completions/inifra.fish"),
        Shell::PowerShell => config.join("inifra/completions/inifra.ps1"),
        Shell::Zsh => data.join("zsh/site-functions/_inifra"),
        _ => return Err(format!("unsupported completion shell {shell}").into()),
    })
}

pub(super) fn activation_instruction(shell: Shell, path: &Path) -> Option<String> {
    let path = path.display().to_string();
    Some(match shell {
        Shell::Fish => return None,
        Shell::Bash => format!(
            "If bash-completion is not loaded, add `source {}` to ~/.bashrc.",
            shell_quote(&path)
        ),
        Shell::Elvish => "Add `use inifra` to your Elvish rc file.".to_owned(),
        Shell::Zsh => format!(
            "Add `fpath=({} $fpath)` before compinit in ~/.zshrc.",
            shell_quote(
                &Path::new(&path)
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .display()
                    .to_string()
            )
        ),
        Shell::PowerShell => format!(
            "Add `. '{}'` to your PowerShell profile.",
            path.replace('\'', "''")
        ),
        _ => return None,
    })
}

pub(super) fn deactivation_instruction(shell: Shell, path: &Path) -> Option<String> {
    let path = path.display().to_string();
    Some(match shell {
        Shell::Fish => return None,
        Shell::Bash => format!(
            "remove any `source {}` line you added to ~/.bashrc",
            shell_quote(&path)
        ),
        Shell::Elvish => "remove `use inifra` from your Elvish rc file if you added it".to_owned(),
        Shell::PowerShell => format!(
            "remove `. '{}'` from your PowerShell profile if you added it",
            path.replace('\'', "''")
        ),
        Shell::Zsh => "the optional fpath entry can remain, or be removed if unused".to_owned(),
        _ => return None,
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn write_script(path: &Path, contents: &[u8], overwrite: bool) -> io::Result<bool> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "completion path has no parent")
        })?;
    fs::create_dir_all(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing to replace completion symlink {}", path.display()),
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "completion target must be a regular file: {}",
                    path.display()
                ),
            ));
        }
        Ok(_) => {
            let existing = fs::read(path)?;
            if existing == contents {
                return Ok(false);
            }

            if !is_generated_script(&existing) && !overwrite {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "refusing to overwrite an unmanaged completion file at {}; rerun with --install --overwrite to replace it",
                        path.display(),
                    ),
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| io::Error::other(error.error.to_string()))?;
    Ok(true)
}

pub(super) fn uninstall_script(path: &Path, force: bool) -> io::Result<bool> {
    let contents = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing to remove completion symlink {}", path.display()),
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "completion target must be a regular file: {}",
                    path.display()
                ),
            ));
        }
        Ok(_) => fs::read(path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };

    if !is_generated_script(&contents) && !force {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "refusing to remove a completion file not marked as Inifra-generated: {}; rerun with --uninstall --force to remove it",
                path.display(),
            ),
        ));
    }
    fs::remove_file(path)?;
    Ok(true)
}

pub(super) fn is_generated_script(contents: &[u8]) -> bool {
    let first_line = contents
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    first_line == COMPLETION_MARKER.as_bytes()
        || first_line.strip_suffix(b"\r") == Some(COMPLETION_MARKER.as_bytes())
}
