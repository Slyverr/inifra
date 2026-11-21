use clap::Args;
use clap_complete::Shell;

mod dynamic;
mod install;

pub(crate) use dynamic::{
    input_completer, preset_completer, source_name_completer, template_completer,
};
pub(crate) use install::run;

pub(crate) const COMPLETION_ENV: &str = "INIFRA_COMPLETE";

#[derive(Debug, Args)]
pub(crate) struct CompletionsArgs {
    /// Shell that should receive completions.
    #[arg(value_enum)]
    pub(crate) shell: Shell,
    /// Install completions in the shell's user completion directory.
    #[arg(long, conflicts_with = "uninstall")]
    pub(crate) install: bool,
    /// Remove Inifra's generated completion file.
    #[arg(long)]
    pub(crate) uninstall: bool,
    /// Replace an existing completion file not marked as Inifra-generated.
    #[arg(long, requires = "install", conflicts_with = "uninstall")]
    pub(crate) overwrite: bool,
    /// Remove an existing completion file not marked as Inifra-generated.
    #[arg(long, requires = "uninstall", conflicts_with = "install")]
    pub(crate) force: bool,
}

#[cfg(test)]
use dynamic::{input_assignments, preset_ids, source_names, template_ids};
#[cfg(test)]
use install::{
    COMPLETION_MARKER, activation_instruction, deactivation_instruction, registration_script,
    shell_completion_path, uninstall_script, user_root, write_script,
};

#[cfg(test)]
mod tests {
    use super::*;
    use inifra_core::presets;
    use std::{collections::BTreeMap, fs, io, path::Path};
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

        assert!(write_script(&target, script, false).unwrap());
        assert!(!write_script(&target, script, false).unwrap());
        assert_eq!(fs::read(&target).unwrap(), script);

        fs::write(&target, "user-owned file\n").unwrap();
        let error = write_script(&target, script, false).unwrap_err();
        assert!(error.to_string().contains("unmanaged"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "user-owned file\n");
        assert!(write_script(&target, script, true).unwrap());
        assert_eq!(fs::read(&target).unwrap(), script);
    }

    #[test]
    fn uninstall_removes_only_marked_completion_files() {
        let directory = TempDir::new().unwrap();
        let target = directory.path().join("inifra.fish");
        fs::write(&target, format!("{COMPLETION_MARKER}\ncomplete inifra\n")).unwrap();

        assert!(uninstall_script(&target, false).unwrap());
        assert!(!uninstall_script(&target, false).unwrap());
    }

    #[test]
    fn embedded_marker_does_not_claim_an_unmanaged_file() {
        let directory = TempDir::new().unwrap();
        let target = directory.path().join("inifra.fish");
        let original = format!("# User completion\n{COMPLETION_MARKER}\ncomplete custom\n");
        fs::write(&target, &original).unwrap();

        let install_error = write_script(&target, b"# replacement\n", false).unwrap_err();
        assert_eq!(install_error.kind(), io::ErrorKind::AlreadyExists);

        let uninstall_error = uninstall_script(&target, false).unwrap_err();
        assert_eq!(uninstall_error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read_to_string(target).unwrap(), original);
    }

    #[test]
    fn dynamic_candidates_follow_catalog_inputs_and_saved_presets() {
        let directory = TempDir::new().unwrap();
        let (catalog, _bundled) = crate::config::user::load_catalog(None, Some(directory.path()))
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

    #[test]
    fn source_name_candidates_follow_the_project_registry() {
        let mut registry = inifra_core::source_registry::SourceRegistry::default();
        let source = inifra_core::source_registry::GitSource {
            url: "https://example.com/team.git".into(),
            reference: "refs/heads/main".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
        };
        registry.insert("team", source.clone()).unwrap();
        registry.insert("team-tools", source).unwrap();

        assert_eq!(source_names(&registry, "team-"), ["team-tools"]);
        assert!(source_names(&registry, "other").is_empty());
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

        assert!(write_script(&target, b"new\n", true).is_err());
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep\n");
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_refuses_to_remove_a_symlink_even_when_forced() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new().unwrap();
        let outside = directory.path().join("outside");
        let target = directory.path().join("inifra.fish");
        fs::write(&outside, "keep\n").unwrap();
        symlink(&outside, &target).unwrap();

        assert!(uninstall_script(&target, true).is_err());
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep\n");
        assert!(
            fs::symlink_metadata(target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
