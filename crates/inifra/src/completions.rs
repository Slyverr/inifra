use clap_complete::{ArgValueCompleter, CompletionCandidate, Shell, env::Shells};
use inifra_core::{catalog::Catalog, presets};
use std::{
    collections::BTreeSet,
    error::Error,
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub(crate) const COMPLETION_ENV: &str = "INIFRA_COMPLETE";
const COMPLETION_MARKER: &str = "# Inifra generated completion file";

pub(crate) fn template_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(prefix) = current.to_str() else {
            return Vec::new();
        };
        let Ok(config_dir) = crate::user_config::config_dir() else {
            return Vec::new();
        };
        let Ok(project_dir) = std::env::current_dir() else {
            return Vec::new();
        };
        let Ok((catalog, _bundled)) =
            crate::user_config::load_project_catalog(None, config_dir.as_deref(), &project_dir)
        else {
            return Vec::new();
        };
        template_ids(&catalog, prefix)
            .into_iter()
            .map(CompletionCandidate::new)
            .collect()
    })
}

pub(crate) fn preset_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(prefix) = current.to_str() else {
            return Vec::new();
        };
        let Ok(config_dir) = crate::user_config::config_dir() else {
            return Vec::new();
        };
        let Some(config_dir) = config_dir else {
            return Vec::new();
        };
        let Ok(project_dir) = std::env::current_dir() else {
            return Vec::new();
        };
        let Ok((catalog, _bundled)) =
            crate::user_config::load_project_catalog(None, Some(&config_dir), &project_dir)
        else {
            return Vec::new();
        };
        preset_ids(&config_dir, &catalog, prefix)
            .into_iter()
            .map(CompletionCandidate::new)
            .collect()
    })
}

pub(crate) fn input_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(prefix) = current.to_str() else {
            return Vec::new();
        };
        let Ok(config_dir) = crate::user_config::config_dir() else {
            return Vec::new();
        };
        let Ok(project_dir) = std::env::current_dir() else {
            return Vec::new();
        };
        let Ok((catalog, _bundled)) =
            crate::user_config::load_project_catalog(None, config_dir.as_deref(), &project_dir)
        else {
            return Vec::new();
        };
        input_assignments(&catalog, prefix)
            .into_iter()
            .map(CompletionCandidate::new)
            .collect()
    })
}

pub(crate) fn run(shell: Shell, install: bool, uninstall: bool) -> Result<(), Box<dyn Error>> {
    validate_shell(shell)?;
    let path = if install || uninstall {
        Some(install_path(shell)?)
    } else {
        None
    };
    if uninstall {
        let path = path.as_ref().expect("an install path was resolved");
        if uninstall_script(path)? {
            println!(
                "{} {}",
                inifra_tui::presentation::paint(
                    "Removed completions:",
                    inifra_tui::presentation::Tone::Success,
                    false
                ),
                inifra_tui::presentation::terminal_text(&path.display().to_string())
            );
            if let Some(instruction) = deactivation_instruction(shell, path) {
                println!(
                    "{} {}",
                    inifra_tui::presentation::paint(
                        "Also remove:",
                        inifra_tui::presentation::Tone::Muted,
                        false
                    ),
                    inifra_tui::presentation::terminal_text(&instruction)
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
        let changed = write_script(path, script.as_bytes())?;
        let action = if changed {
            "Wrote completion file:"
        } else {
            "Completion file is up to date:"
        };
        println!(
            "{} {}",
            inifra_tui::presentation::paint(action, inifra_tui::presentation::Tone::Success, false),
            inifra_tui::presentation::terminal_text(&path.display().to_string())
        );
        if let Some(instruction) = activation_instruction(shell, path) {
            println!(
                "{} {}",
                inifra_tui::presentation::paint(
                    "To activate:",
                    inifra_tui::presentation::Tone::Muted,
                    false
                ),
                inifra_tui::presentation::terminal_text(&instruction)
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

fn registration_script(shell: Shell) -> Result<String, Box<dyn Error>> {
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

fn install_path(shell: Shell) -> Result<PathBuf, Box<dyn Error>> {
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

fn user_root(
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

fn shell_completion_path(
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

fn activation_instruction(shell: Shell, path: &Path) -> Option<String> {
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

fn deactivation_instruction(shell: Shell, path: &Path) -> Option<String> {
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

fn write_script(path: &Path, contents: &[u8]) -> io::Result<bool> {
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
            if !is_generated_script(&existing) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "refusing to overwrite an unmanaged completion file at {}",
                        path.display()
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

fn uninstall_script(path: &Path) -> io::Result<bool> {
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
    if !is_generated_script(&contents) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "refusing to remove a completion file not marked as Inifra-generated: {}",
                path.display()
            ),
        ));
    }
    fs::remove_file(path)?;
    Ok(true)
}

fn is_generated_script(contents: &[u8]) -> bool {
    let first_line = contents
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    first_line == COMPLETION_MARKER.as_bytes()
        || first_line.strip_suffix(b"\r") == Some(COMPLETION_MARKER.as_bytes())
}

fn template_ids(catalog: &Catalog, prefix: &str) -> Vec<String> {
    catalog
        .packs
        .iter()
        .filter(|pack| pack.id.starts_with(prefix))
        .map(|pack| pack.id.clone())
        .collect()
}

fn preset_ids(config_dir: &Path, catalog: &Catalog, prefix: &str) -> Vec<String> {
    presets::load_all(&presets::store_path(config_dir), catalog)
        .unwrap_or_default()
        .into_iter()
        .map(|preset| preset.id)
        .filter(|id| id.starts_with(prefix))
        .collect()
}

fn input_assignments(catalog: &Catalog, prefix: &str) -> Vec<String> {
    catalog
        .packs
        .iter()
        .filter_map(|pack| catalog.input_definitions_for(pack).ok())
        .flat_map(|definitions| definitions.into_keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| format!("{name}="))
        .filter(|name| name.starts_with(prefix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, fs};
    use tempfile::TempDir;

    #[test]
    fn generated_shell_scripts_register_dynamic_completion() {
        for shell in [
            Shell::Bash,
            Shell::Elvish,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Zsh,
        ] {
            let script = registration_script(shell).expect("generate shell completion");
            assert!(script.starts_with(COMPLETION_MARKER));
            assert!(script.contains(COMPLETION_ENV));
            assert!(script.contains("inifra"));
        }
    }

    #[test]
    fn activation_instructions_match_shell_loading_behavior() {
        let directory = TempDir::new().unwrap();
        let path = directory
            .path()
            .join(".local/share/zsh/site-functions/_inifra");
        assert!(activation_instruction(Shell::Fish, &path).is_none());
        let zsh_instruction = activation_instruction(Shell::Zsh, &path).unwrap();
        let zsh_directory = path.parent().unwrap().display().to_string();
        assert!(zsh_instruction.contains(&format!("fpath=('{zsh_directory}' $fpath)")));
        assert!(
            activation_instruction(Shell::Elvish, &path)
                .unwrap()
                .contains("use inifra")
        );
        assert!(
            activation_instruction(Shell::PowerShell, &path)
                .unwrap()
                .contains("PowerShell profile")
        );
    }

    #[test]
    fn deactivation_instructions_explain_manual_cleanup() {
        let directory = TempDir::new().unwrap();
        let bash_path = directory
            .path()
            .join(".local/share/bash-completion/completions/inifra.bash");
        let elvish_path = directory.path().join(".config/elvish/lib/inifra.elv");
        let powershell_path = directory.path().join("inifra/completions/inifra.ps1");

        assert!(deactivation_instruction(Shell::Fish, &bash_path).is_none());
        assert!(
            deactivation_instruction(Shell::Bash, &bash_path)
                .unwrap()
                .contains("remove any `source")
        );
        assert!(
            deactivation_instruction(Shell::Elvish, &elvish_path)
                .unwrap()
                .contains("remove `use inifra`")
        );
        let powershell_instruction =
            deactivation_instruction(Shell::PowerShell, &powershell_path).unwrap();
        assert!(
            powershell_instruction.contains(&format!("remove `. '{}'`", powershell_path.display()))
        );
        assert!(
            deactivation_instruction(Shell::Zsh, &bash_path)
                .unwrap()
                .contains("fpath")
        );
    }

    #[test]
    fn completion_roots_follow_xdg_overrides_and_platform_fallbacks() {
        let directory = TempDir::new().unwrap();
        let home = directory.path().join("home");
        let platform_config = directory.path().join("platform/config");
        let platform_data = directory.path().join("platform/data");
        let custom_config = directory.path().join("custom/config");

        assert_eq!(
            user_root(
                Shell::Bash,
                Some(custom_config.clone().into_os_string()),
                &home,
                &platform_config,
                Path::new(".config"),
                false,
            )
            .unwrap(),
            custom_config
        );
        assert_eq!(
            user_root(
                Shell::Bash,
                None,
                &home,
                &platform_config,
                Path::new(".config"),
                false,
            )
            .unwrap(),
            home.join(".config")
        );
        assert_eq!(
            user_root(
                Shell::Zsh,
                None,
                &home,
                &platform_data,
                Path::new(".local/share"),
                false,
            )
            .unwrap(),
            home.join(".local/share")
        );
        assert_eq!(
            user_root(
                Shell::Bash,
                None,
                &home,
                &platform_config,
                Path::new(".config"),
                true,
            )
            .unwrap(),
            home.join(".config")
        );
        assert_eq!(
            user_root(
                Shell::PowerShell,
                None,
                &home,
                &platform_config,
                Path::new(".config"),
                true,
            )
            .unwrap(),
            platform_config
        );
        assert_eq!(
            user_root(
                Shell::PowerShell,
                None,
                &home,
                &platform_data,
                Path::new(".local/share"),
                true,
            )
            .unwrap(),
            platform_data
        );
    }

    #[test]
    fn relative_xdg_roots_are_rejected() {
        let directory = TempDir::new().unwrap();
        let error = user_root(
            Shell::Bash,
            Some("relative/config".into()),
            directory.path(),
            directory.path(),
            Path::new(".config"),
            false,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn completion_paths_use_each_shells_user_directory_convention() {
        let directory = TempDir::new().unwrap();
        let config = directory.path().join(".config");
        let data = directory.path().join(".local/share");

        assert_eq!(
            shell_completion_path(Shell::Bash, &config, &data).unwrap(),
            data.join("bash-completion/completions/inifra.bash")
        );
        assert_eq!(
            shell_completion_path(Shell::Elvish, &config, &data).unwrap(),
            config.join("elvish/lib/inifra.elv")
        );
        assert_eq!(
            shell_completion_path(Shell::Fish, &config, &data).unwrap(),
            config.join("fish/completions/inifra.fish")
        );
        assert_eq!(
            shell_completion_path(Shell::PowerShell, &config, &data).unwrap(),
            config.join("inifra/completions/inifra.ps1")
        );
        assert_eq!(
            shell_completion_path(Shell::Zsh, &config, &data).unwrap(),
            data.join("zsh/site-functions/_inifra")
        );
    }

    #[test]
    fn install_is_idempotent_and_preserves_unmanaged_files() {
        let directory = TempDir::new().unwrap();
        let target = directory.path().join("fish/completions/inifra.fish");
        let script = b"# Inifra generated completion file\ncomplete inifra\n";

        assert!(write_script(&target, script).unwrap());
        assert!(!write_script(&target, script).unwrap());
        assert_eq!(fs::read(&target).unwrap(), script);

        fs::write(&target, "user-owned file\n").unwrap();
        let error = write_script(&target, script).unwrap_err();
        assert!(error.to_string().contains("unmanaged"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "user-owned file\n");
    }

    #[test]
    fn uninstall_removes_only_marked_completion_files() {
        let directory = TempDir::new().unwrap();
        let target = directory.path().join("inifra.fish");
        fs::write(&target, format!("{COMPLETION_MARKER}\ncomplete inifra\n")).unwrap();

        assert!(uninstall_script(&target).unwrap());
        assert!(!uninstall_script(&target).unwrap());
    }

    #[test]
    fn embedded_marker_does_not_claim_an_unmanaged_file() {
        let directory = TempDir::new().unwrap();
        let target = directory.path().join("inifra.fish");
        let original = format!("# User completion\n{COMPLETION_MARKER}\ncomplete custom\n");
        fs::write(&target, &original).unwrap();

        let install_error = write_script(&target, b"# replacement\n").unwrap_err();
        assert_eq!(install_error.kind(), io::ErrorKind::AlreadyExists);

        let uninstall_error = uninstall_script(&target).unwrap_err();
        assert_eq!(uninstall_error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read_to_string(target).unwrap(), original);
    }

    #[test]
    fn dynamic_candidates_follow_catalog_inputs_and_saved_presets() {
        let directory = TempDir::new().unwrap();
        let (catalog, _bundled) = crate::user_config::load_catalog(None, Some(directory.path()))
            .expect("load bundled catalog");

        assert_eq!(template_ids(&catalog, "read"), ["readme"]);
        assert_eq!(
            input_assignments(&catalog, "project_name"),
            ["project_name="]
        );

        let preset = presets::Preset {
            id: "starter".into(),
            description: "Starter selection".into(),
            templates: vec!["agents".into()],
            values: BTreeMap::new(),
        };
        presets::save(
            &presets::store_path(directory.path()),
            &preset,
            &catalog,
            false,
        )
        .expect("save preset");

        assert_eq!(preset_ids(directory.path(), &catalog, "start"), ["starter"]);
    }

    #[cfg(unix)]
    #[test]
    fn install_refuses_to_follow_a_symlink() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new().unwrap();
        let outside = directory.path().join("outside");
        let target = directory.path().join("inifra.fish");
        fs::write(&outside, "keep\n").unwrap();
        symlink(&outside, &target).unwrap();

        assert!(write_script(&target, b"new\n").is_err());
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep\n");
    }
}
