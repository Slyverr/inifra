use super::*;

#[test]
fn completions_generate_dynamic_scripts_for_each_supported_shell() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let output = run_cli(&["completions", shell]);

        assert!(
            output.status.success(),
            "completion generation failed for {shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("inifra"),
            "completion output for {shell} did not register Inifra"
        );
    }
}

#[test]
fn completions_install_for_each_supported_shell_without_editing_startup_files() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let config = tempdir().expect("create isolated config directory");
        let output = run_cli_with_config(&["completions", shell, "--install"], config.path());
        let path = installed_completion_path(config.path(), shell);

        assert!(
            output.status.success(),
            "completion install failed for {shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(path.is_file(), "completion file missing for {shell}");
        assert!(
            !std::fs::read(path)
                .expect("read installed completions")
                .is_empty()
        );
        for startup_file in [
            ".bashrc",
            ".zshrc",
            "config.fish",
            "Microsoft.PowerShell_profile.ps1",
        ] {
            assert!(
                !config.path().join(startup_file).exists(),
                "installer unexpectedly created {startup_file}"
            );
        }
    }
}

#[test]
fn completions_install_keeps_the_previous_apply_spelling() {
    let config = tempdir().expect("create isolated config directory");
    let output = run_cli_with_config(&["completions", "fish", "--apply"], config.path());

    assert!(output.status.success());
    assert!(fish_completion_path(config.path()).is_file());
}

#[cfg(unix)]
#[test]
fn completions_install_rejects_a_symlink_destination() {
    use std::os::unix::fs::symlink;

    let config = tempdir().expect("create isolated config directory");
    let directory = config.path().join("inifra/completions");
    std::fs::create_dir_all(&directory).expect("create completion directory");
    let outside = config.path().join("outside");
    std::fs::write(&outside, "keep me").expect("create outside file");
    symlink(&outside, directory.join("inifra.bash")).expect("create destination symlink");
    let output = run_cli_with_config(&["completions", "bash", "--install"], config.path());

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));
    assert_eq!(
        std::fs::read_to_string(outside).expect("read outside file"),
        "keep me"
    );
}

#[test]
fn fish_dynamic_completions_include_matching_template_ids() {
    let config = tempdir().expect("create isolated config directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args(["--", "inifra", "generate", "rea"])
        .env("COMPLETE", "fish");
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run dynamic completion");

    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line == "readme")
    );
}

#[test]
fn completions_include_project_templates_for_the_requested_destination() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "team-guide",
        "Team guide",
        "TEAM.md",
        "team\n",
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args([
            "--",
            "inifra",
            "generate",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
            "team",
        ])
        .env("COMPLETE", "fish")
        .env("_CLAP_COMPLETE_INDEX", "4");
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run project template completion");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("team-guide"),
        "completion output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn all_shells_dynamically_complete_template_ids_and_shared_input_names() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let config = tempdir().expect("create isolated config directory");
        let mut templates = Command::new(env!("CARGO_BIN_EXE_inifra"));
        templates
            .args(["--", "inifra", "generate", "rea"])
            .env("COMPLETE", shell)
            .env("_CLAP_COMPLETE_INDEX", "2");
        set_config_directory(&mut templates, config.path());
        let output = templates.output().expect("run template completion");

        assert!(
            output.status.success(),
            "template completion failed for {shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("readme"),
            "template ID was not completed for {shell}"
        );

        write_local_pack(
            config.path(),
            "custom-template",
            "A local starter",
            "docs/START.md",
            "# Starter\n",
        );
        let mut local_templates = Command::new(env!("CARGO_BIN_EXE_inifra"));
        local_templates
            .args(["--", "inifra", "generate", "custom"])
            .env("COMPLETE", shell)
            .env("_CLAP_COMPLETE_INDEX", "2");
        set_config_directory(&mut local_templates, config.path());
        let output = local_templates
            .output()
            .expect("run local template completion");

        assert!(
            output.status.success(),
            "local template completion failed for {shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("custom-template"));

        let mut inputs = Command::new(env!("CARGO_BIN_EXE_inifra"));
        inputs
            .args(["--", "inifra", "generate", "readme", "--set", "pro"])
            .env("COMPLETE", shell)
            .env("_CLAP_COMPLETE_INDEX", "4");
        set_config_directory(&mut inputs, config.path());
        let output = inputs.output().expect("run input completion");

        assert!(
            output.status.success(),
            "input completion failed for {shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("project_name="),
            "shared input name was not completed for {shell}"
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("copyright_year="));
    }
}

#[test]
fn every_shell_dynamically_completes_saved_preset_names() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let config = tempdir().expect("create isolated config directory");
        let saved = run_cli_with_config(
            &["preset", "save", "demo-setup", "--template", "readme"],
            config.path(),
        );
        assert!(saved.status.success());

        let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
        command
            .args(["--", "inifra", "init", "--preset", "demo"])
            .env("COMPLETE", shell)
            .env("_CLAP_COMPLETE_INDEX", "3");
        set_config_directory(&mut command, config.path());
        let output = command.output().expect("run dynamic completion");

        assert!(
            output.status.success(),
            "preset completion failed for {shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.split('\t').next() == Some("demo-setup")),
            "saved preset was not completed for {shell}"
        );
    }
}

fn fish_completion_path(config: &std::path::Path) -> std::path::PathBuf {
    config.join("fish/completions/inifra.fish")
}

fn installed_completion_path(config: &std::path::Path, shell: &str) -> std::path::PathBuf {
    match shell {
        "fish" => fish_completion_path(config),
        "bash" => config.join("inifra/completions/inifra.bash"),
        "elvish" => config.join("inifra/completions/inifra.elv"),
        "powershell" => config.join("inifra/completions/inifra.ps1"),
        "zsh" => config.join("inifra/completions/_inifra"),
        _ => panic!("unsupported shell {shell}"),
    }
}
