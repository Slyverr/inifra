use std::process::{Command, Output};

use tempfile::tempdir;

fn run_cli(arguments: &[&str]) -> Output {
    let config = tempdir().expect("create isolated config directory");
    run_cli_with_config(arguments, config.path())
}

fn run_cli_in(arguments: &[&str], directory: &std::path::Path) -> Output {
    let config = tempdir().expect("create isolated config directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.args(arguments).current_dir(directory);
    set_config_directory(&mut command, config.path());
    command.output().expect("run Inifra")
}

fn run_cli_with_config(arguments: &[&str], config: &std::path::Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.args(arguments);
    set_config_directory(&mut command, config);
    command.output().expect("run Inifra")
}

fn run_cli_with_forced_colors(arguments: &[&str], no_color: bool) -> Output {
    let config = tempdir().expect("create isolated config directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args(arguments)
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR");
    if no_color {
        command.env("NO_COLOR", "1");
    }
    set_config_directory(&mut command, config.path());
    command.output().expect("run Inifra")
}

fn set_config_directory(command: &mut Command, config: &std::path::Path) {
    command.env("INIFRA_CONFIG_DIR", config);
}

fn write_local_pack(
    config: &std::path::Path,
    id: &str,
    description: &str,
    output: &str,
    body: &str,
) {
    let pack = templates_root(config).join(id);
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(
        pack.join("template.toml"),
        format!(
            "schema_version = 1\nid = {id:?}\ndescription = {description:?}\n\n[[files]]\nsource = \"source.txt\"\noutput = {output:?}\n"
        ),
    )
    .expect("write manifest");
    std::fs::write(pack.join("source.txt"), body).expect("write template source");
}

fn templates_root(config: &std::path::Path) -> std::path::PathBuf {
    config.join("inifra/templates")
}

fn write_project_pack(
    project: &std::path::Path,
    id: &str,
    description: &str,
    output: &str,
    body: &str,
) {
    let pack = project.join(".inifra/templates").join(id);
    std::fs::create_dir_all(&pack).expect("create project template pack");
    std::fs::write(
        pack.join("template.toml"),
        format!(
            "schema_version = 1\nid = {id:?}\ndescription = {description:?}\n\n[[files]]\nsource = \"source.txt\"\noutput = {output:?}\n"
        ),
    )
    .expect("write project template manifest");
    std::fs::write(pack.join("source.txt"), body).expect("write project template source");
}

fn write_project_inputs(project: &std::path::Path, name: &str, default: &str) {
    let config = project.join(".inifra");
    std::fs::create_dir_all(&config).expect("create project Inifra config");
    std::fs::write(
        config.join("inputs.toml"),
        format!(
            "schema_version = 1\n\n[inputs.{name}]\nprompt = {name:?}\ndefault = {default:?}\n"
        ),
    )
    .expect("write project input definitions");
}

#[test]
fn help_lists_available_commands() {
    let output = run_cli(&["--help"]);

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("list"));
    assert!(help.contains("validate"));
    assert!(help.contains("generate"));
    assert!(help.contains("init"));
    assert!(help.contains("preset"));
    assert!(help.contains("source"));
    assert!(help.contains("completions"));
}

#[test]
fn validate_checks_all_bundled_templates_without_creating_config_files() {
    let config = tempdir().expect("create isolated config directory");
    let output = run_cli_with_config(&["validate"], config.path());

    assert!(
        output.status.success(),
        "validation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("valid agents"));
    assert!(stdout.contains("valid readme"));
    assert!(!config.path().join("inifra").exists());
}

#[test]
fn validate_filters_by_template_id() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "notes",
        "Project notes",
        "NOTES.md",
        "Notes\n",
    );
    let output = run_cli_with_config(&["validate", "notes"], config.path());

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "valid notes"
    );
}

#[test]
fn validate_rejects_malformed_local_manifests() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("broken");
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(pack.join("template.toml"), "schema_version = [")
        .expect("write malformed manifest");

    let output = run_cli_with_config(&["validate"], config.path());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("template.toml"));
    assert!(stderr.contains("could not parse template manifest"));
}

#[test]
fn validate_rejects_template_inputs_without_shared_definitions() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("broken");
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"broken\"\ndescription = \"Broken template\"\ninputs = [\"missing_input\"]\n\n[[files]]\nsource = \"README.md.j2\"\noutput = \"README.md\"\n",
    )
    .expect("write manifest");
    std::fs::write(pack.join("README.md.j2"), "# Example\n").expect("write template source");

    let output = run_cli_with_config(&["validate", "broken"], config.path());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("missing_input"));
    assert!(stderr.contains("undefined input"));
}

#[test]
fn validate_reports_undefined_values_with_the_template_and_output_path() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("broken");
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"broken\"\ndescription = \"Broken template\"\n\n[[files]]\nsource = \"README.md.j2\"\noutput = \"README.md\"\n",
    )
    .expect("write manifest");
    std::fs::write(pack.join("README.md.j2"), "# {{ missing_value }}\n")
        .expect("write template source");

    let output = run_cli_with_config(&["validate", "broken"], config.path());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("broken"));
    assert!(stderr.contains("README.md"));
    assert!(stderr.contains("undefined"));
    assert!(!config.path().join("inifra/inputs.toml").exists());
}

#[test]
fn validate_reports_missing_template_sources() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("broken");
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"broken\"\ndescription = \"Broken template\"\n\n[[files]]\nsource = \"missing.md\"\noutput = \"README.md\"\n",
    )
    .expect("write manifest");

    let output = run_cli_with_config(&["validate"], config.path());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("missing.md"));
}

#[test]
fn human_readable_output_can_be_forced_to_color() {
    let output = run_cli_with_forced_colors(&["list"], false);

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("\u{1b}["));
}

#[test]
fn human_readable_output_is_plain_when_redirected() {
    let output = run_cli(&["list"]);

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("\u{1b}["));
}

#[test]
fn no_color_disables_forced_human_readable_colors() {
    let output = run_cli_with_forced_colors(&["list"], true);

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("\u{1b}["));
}

#[test]
fn errors_use_color_when_forced() {
    let output = run_cli_with_forced_colors(&["generate", "missing-template"], false);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("\u{1b}["));
}

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

#[cfg(unix)]
#[test]
fn generate_edit_opens_text_before_writing() {
    use std::os::unix::fs::PermissionsExt;

    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    let editor = config.path().join("fake-editor");
    std::fs::write(&editor, "#!/bin/sh\nprintf '# Edited\\n' > \"$1\"\n")
        .expect("write fake editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args([
            "generate",
            "readme",
            "--path",
            project.path().to_str().expect("project path is UTF-8"),
            "--set",
            "project_name=Example",
            "--edit",
        ])
        .env("VISUAL", &editor);
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run Inifra");

    assert!(output.status.success());
    assert_eq!(
        std::fs::read(project.path().join("README.md")).expect("read generated README"),
        b"# Edited\n"
    );
}

#[cfg(unix)]
#[test]
fn edit_copies_builtin_template_to_user_config_before_opening() {
    use std::os::unix::fs::PermissionsExt;

    let config = tempdir().expect("create isolated config directory");
    let editor = config.path().join("fake-editor");
    std::fs::write(
        &editor,
        "#!/bin/sh\nprintf '\\n<!-- customized -->\\n' >> \"$2\"\n",
    )
    .expect("write fake editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.args(["edit", "readme"]).env("VISUAL", &editor);
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run Inifra");
    let template_source = config.path().join("inifra/templates/readme/README.md.j2");

    assert!(output.status.success());
    assert!(template_source.is_file());
    assert!(
        std::fs::read_to_string(template_source)
            .expect("read edited local template")
            .contains("<!-- customized -->")
    );
}

#[cfg(unix)]
#[test]
fn edit_opens_the_project_local_template_when_given_a_project_path() {
    use std::os::unix::fs::PermissionsExt;

    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "team-guide",
        "Team guide",
        "TEAM.md",
        "team\n",
    );
    let editor = config.path().join("fake-editor");
    std::fs::write(&editor, "#!/bin/sh\nprintf 'edited\\n' >> \"$2\"\n")
        .expect("write fake editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args([
            "edit",
            "team-guide",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ])
        .env("VISUAL", &editor);
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run project template editor");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(
            project
                .path()
                .join(".inifra/templates/team-guide/source.txt")
        )
        .expect("read edited project template"),
        "team\nedited\n"
    );
    assert!(!templates_root(config.path()).exists());
}

#[test]
fn edit_unknown_template_requires_an_interactive_terminal() {
    let output = run_cli(&["edit", "new-template"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("run `inifra edit new-template` in a terminal")
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

#[test]
fn no_arguments_show_help_as_a_friendly_entry_point() {
    let output = run_cli(&[]);

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Add reusable starter files to a project"));
    assert!(help.contains("generate"));
}

#[test]
fn list_shows_bundled_templates_and_output_paths() {
    let output = run_cli(&["list"]);

    assert!(output.status.success());
    let listing = String::from_utf8_lossy(&output.stdout);
    assert!(listing.contains("changelog"));
    assert!(listing.contains("CHANGELOG.md"));
    assert!(listing.contains("agents"));
    assert!(listing.contains("docs-architecture"));
    assert!(listing.contains("docs-development"));
    assert!(listing.contains("github-issues"));
    assert!(listing.contains("github-pull-request"));
    assert!(listing.contains("security"));
    assert!(listing.contains("editorconfig"));
    assert!(listing.contains(".editorconfig"));
    assert!(!listing.contains("gitignore"));
    assert!(listing.contains("contributing"));
    assert!(listing.contains("CONTRIBUTING.md"));
    assert!(listing.contains("license-mit"));
    assert!(listing.contains("license-apache"));
    assert!(listing.contains("LICENSE"));
}

#[test]
fn list_discovers_templates_from_the_user_config_directory() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "notes",
        "Project notes",
        "NOTES.md",
        "hello\n",
    );

    let output = run_cli_with_config(&["list"], config.path());

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8_lossy(&output.stdout);
    assert!(listing.contains("notes"));
    assert!(listing.contains("[user-local]"));
    assert!(listing.contains("NOTES.md"));
}

#[test]
fn local_template_with_a_builtin_id_overrides_the_builtin() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "editorconfig",
        "Custom editor settings",
        ".editorconfig",
        "custom\n",
    );
    let destination = tempdir().expect("create destination");

    let listing = run_cli_with_config(&["list"], config.path());
    assert!(listing.status.success());
    let listing = String::from_utf8_lossy(&listing.stdout);
    assert!(listing.contains("Custom editor settings [user-local]"));
    assert!(!listing.contains("Portable editor formatting defaults [built-in]"));

    let output = run_cli_with_config(
        &[
            "generate",
            "editorconfig",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(destination.path().join(".editorconfig")).unwrap(),
        "custom\n"
    );
}

#[test]
fn duplicate_local_template_ids_are_reported() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "notes",
        "Project notes",
        "NOTES.md",
        "first\n",
    );
    let duplicate = templates_root(config.path()).join("another-pack");
    std::fs::create_dir_all(&duplicate).expect("create duplicate pack");
    std::fs::write(
        duplicate.join("template.toml"),
        "schema_version = 1\nid = \"notes\"\ndescription = \"Duplicate notes\"\n\n[[files]]\nsource = \"source.txt\"\noutput = \"OTHER.md\"\n",
    )
    .expect("write duplicate manifest");
    std::fs::write(duplicate.join("source.txt"), "second\n").expect("write source");

    let output = run_cli_with_config(&["list"], config.path());

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("multiple local templates use the same ID")
    );
}

#[test]
fn malformed_local_manifests_report_an_error() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("broken");
    std::fs::create_dir_all(&pack).expect("create malformed pack");
    std::fs::write(pack.join("template.toml"), "not = valid = toml").expect("write manifest");

    let output = run_cli_with_config(&["list"], config.path());

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("invalid template manifest"));
    assert!(error.contains("template.toml"));
}

#[test]
fn generate_renders_jinja_text_sources() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("rendered");
    std::fs::create_dir_all(&pack).expect("create template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"rendered\"\ndescription = \"Rendered text\"\n\n[[files]]\nsource = \"message.txt.j2\"\noutput = \"message.txt\"\n",
    )
    .expect("write manifest");
    std::fs::write(pack.join("message.txt.j2"), "{{ 1 + 1 }}\n").expect("write source");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "generate",
            "rendered",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(destination.path().join("message.txt")).unwrap(),
        "2\n"
    );
}

#[test]
fn readme_template_uses_shared_inputs() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "generate",
            "readme",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
            "--set",
            "project_name=Inifra",
            "--set",
            "project_description=A template-driven project setup tool.",
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let readme = std::fs::read_to_string(destination.path().join("README.md")).unwrap();
    assert!(
        readme.starts_with("# Inifra\n\nA template-driven project setup tool."),
        "unexpected README output: {readme:?}"
    );
    assert!(readme.contains("## Getting started"));
    assert!(readme.contains("## Usage"));
    assert!(readme.contains("## Support"));
}

#[test]
fn contributing_template_uses_the_shared_project_name() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    let output = run_cli_with_config(
        &[
            "generate",
            "contributing",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
            "--set",
            "project_name=Example Project",
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let contributing = std::fs::read_to_string(destination.path().join("CONTRIBUTING.md"))
        .expect("read generated contributor guide");
    assert!(contributing.starts_with("# Contributing to Example Project\n"));
    assert!(contributing.contains("## Development setup"));
    assert!(contributing.contains("## Checks"));
}

#[test]
fn readme_and_contributing_reuse_one_project_name_input() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    let output = run_cli_with_config(
        &[
            "init",
            "--template",
            "readme",
            "--template",
            "contributing",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
            "--set",
            "project_name=Shared name",
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let readme = std::fs::read_to_string(destination.path().join("README.md"))
        .expect("read generated README");
    let contributing = std::fs::read_to_string(destination.path().join("CONTRIBUTING.md"))
        .expect("read generated contributor guide");
    assert!(readme.starts_with("# Shared name\n"));
    assert!(contributing.starts_with("# Contributing to Shared name\n"));
}

#[test]
fn mit_license_renders_shared_copyright_inputs() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    let output = run_cli_with_config(
        &[
            "generate",
            "license-mit",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
            "--set",
            "copyright_year=2026",
            "--set",
            "copyright_holder=Slyverr",
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let license = std::fs::read_to_string(destination.path().join("LICENSE"))
        .expect("read generated MIT license");
    assert!(license.starts_with("MIT License\n\nCopyright 2026 Slyverr\n"));
    assert!(license.contains("Permission is hereby granted, free of charge"));
    assert!(!license.contains("{{"));
}

#[test]
fn apache_license_matches_the_bundled_standard_text() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    let output = run_cli_with_config(
        &[
            "generate",
            "license-apache",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let generated =
        std::fs::read(destination.path().join("LICENSE")).expect("read generated Apache license");
    let bundled = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/templates/license-apache/LICENSE"
    ))
    .expect("read bundled Apache license text");
    assert_eq!(generated, bundled);
    let generated = String::from_utf8_lossy(&generated);
    assert!(generated.contains("Apache License"));
    assert!(generated.contains("Version 2.0, January 2004"));
}

#[test]
fn readme_template_detects_cargo_metadata_from_destination() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    std::fs::write(
        destination.path().join("Cargo.toml"),
        "[package]\nname = \"detected-project\"\nversion = \"0.1.0\"\ndescription = \"Detected description\"\nrepository = \"https://example.com/project\"\n",
    )
    .expect("write Cargo manifest");

    let output = run_cli_with_config(
        &[
            "generate",
            "readme",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let readme = std::fs::read_to_string(destination.path().join("README.md")).unwrap();
    assert!(readme.starts_with("# detected-project\n\nDetected description"));
    assert!(readme.contains("https://example.com/project"));
}

#[test]
fn explicit_inputs_override_detected_project_values() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    std::fs::write(
        destination.path().join("package.json"),
        "{\"name\":\"detected-name\",\"description\":\"Detected description\"}",
    )
    .expect("write package manifest");

    let output = run_cli_with_config(
        &[
            "generate",
            "readme",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
            "--set",
            "project_name=Explicit name",
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let readme = std::fs::read_to_string(destination.path().join("README.md")).unwrap();
    assert!(readme.starts_with("# Explicit name\n\nDetected description"));
}

#[test]
fn templates_without_inputs_ignore_unrelated_project_manifest_errors() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    std::fs::write(destination.path().join("Cargo.toml"), "invalid = [toml")
        .expect("write malformed manifest");

    let output = run_cli_with_config(
        &[
            "generate",
            "editorconfig",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(destination.path().join(".editorconfig").is_file());
}

#[test]
fn readme_template_requires_a_project_name_before_writing() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "generate",
            "readme",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--set project_name=VALUE"));
    assert!(!destination.path().join("README.md").exists());
}

#[test]
fn user_input_defaults_are_shared_by_template_renders() {
    let config = tempdir().expect("create isolated config directory");
    let app_config = config.path().join("inifra");
    std::fs::create_dir_all(&app_config).expect("create Inifra config directory");
    std::fs::write(
        app_config.join("inputs.toml"),
        "schema_version = 1\n[inputs.project_name]\nprompt = \"Project name\"\nrequired = true\ndefault = \"Configured name\"\n",
    )
    .expect("write shared input definition");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "generate",
            "readme",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let readme = std::fs::read_to_string(destination.path().join("README.md")).unwrap();
    assert!(readme.starts_with("# Configured name\n"));
}

#[test]
fn duplicate_and_undeclared_input_assignments_are_rejected() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");
    let path = destination.path().to_str().expect("UTF-8 path");

    let duplicate = run_cli_with_config(
        &[
            "generate",
            "readme",
            "--path",
            path,
            "--set",
            "project_name=First",
            "--set",
            "project_name=Second",
        ],
        config.path(),
    );
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("provided more than once"));

    let unknown = run_cli_with_config(
        &[
            "generate",
            "readme",
            "--path",
            path,
            "--set",
            "unknown=value",
        ],
        config.path(),
    );
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("does not use input"));
}

#[test]
fn generate_fails_on_undefined_template_values_before_writing() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("missing-value");
    std::fs::create_dir_all(&pack).expect("create template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"missing-value\"\ndescription = \"Missing value\"\n\n[[files]]\nsource = \"message.txt.j2\"\noutput = \"message.txt\"\n",
    )
    .expect("write manifest");
    std::fs::write(pack.join("message.txt.j2"), "{{ required_value }}\n").expect("write source");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "generate",
            "missing-value",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not render template"));
    assert!(!destination.path().join("message.txt").exists());
}

#[test]
fn list_is_independent_of_the_current_directory() {
    let destination = tempdir().expect("create temporary directory");
    write_project_pack(
        destination.path(),
        "project-only",
        "Project-only template",
        "PROJECT.md",
        "project\n",
    );
    let output = run_cli_in(&["list"], destination.path());

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("project-only"));
    assert!(!destination.path().join("CHANGELOG.md").exists());
}

#[test]
fn list_includes_project_templates_only_when_given_a_project_path() {
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "team-guide",
        "Team guide",
        "TEAM.md",
        "team\n",
    );

    let output = run_cli(&[
        "list",
        "--path",
        project.path().to_str().expect("UTF-8 project path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8_lossy(&output.stdout);
    assert!(listing.contains("team-guide"));
    assert!(listing.contains("[project-local]"));
}

#[test]
fn project_templates_override_user_templates_for_generation() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "editorconfig",
        "User editor settings",
        ".editorconfig",
        "user\n",
    );
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "editorconfig",
        "Project editor settings",
        ".editorconfig",
        "project\n",
    );

    let output = run_cli_with_config(
        &[
            "generate",
            "editorconfig",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join(".editorconfig")).expect("read generated file"),
        "project\n"
    );
}

#[test]
fn validate_includes_project_templates_when_given_a_project_path() {
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "team-guide",
        "Team guide",
        "TEAM.md",
        "team\n",
    );

    let output = run_cli(&[
        "validate",
        "--path",
        project.path().to_str().expect("UTF-8 project path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("valid team-guide"));
}

#[test]
fn project_templates_use_project_shared_input_definitions() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    let user_config = config.path().join("inifra");
    std::fs::create_dir_all(&user_config).expect("create user Inifra config");
    std::fs::write(
        user_config.join("inputs.toml"),
        "schema_version = 1\n\n[inputs.team_name]\nprompt = \"Team name\"\ndefault = \"Personal team\"\n",
    )
    .expect("write user input definitions");
    write_project_inputs(project.path(), "team_name", "Platform team");
    let pack = project.path().join(".inifra/templates/team-guide");
    std::fs::create_dir_all(&pack).expect("create project template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"team-guide\"\ndescription = \"Team guide\"\ninputs = [\"team_name\"]\n\n[[files]]\nsource = \"TEAM.md.j2\"\noutput = \"TEAM.md\"\n",
    )
    .expect("write project template manifest");
    std::fs::write(pack.join("TEAM.md.j2"), "# {{ team_name }}\n")
        .expect("write project template source");

    let output = run_cli_with_config(
        &[
            "generate",
            "team-guide",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("TEAM.md")).expect("read generated file"),
        "# Platform team\n"
    );
    let validation = run_cli_with_config(
        &[
            "validate",
            "team-guide",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ],
        config.path(),
    );
    assert!(
        validation.status.success(),
        "{}",
        String::from_utf8_lossy(&validation.stderr)
    );
}

#[test]
fn project_input_registry_is_ignored_for_templates_that_use_no_inputs() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    let project_config = project.path().join(".inifra");
    std::fs::create_dir_all(&project_config).expect("create project Inifra config");
    std::fs::write(project_config.join("inputs.toml"), "malformed = [")
        .expect("write unrelated malformed input registry");

    let output = run_cli_with_config(
        &[
            "generate",
            "editorconfig",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.path().join(".editorconfig").is_file());
}

#[cfg(unix)]
#[test]
fn project_template_discovery_rejects_a_symlinked_config_directory() {
    use std::os::unix::fs::symlink;

    let project = tempdir().expect("create project directory");
    let outside = tempdir().expect("create outside directory");
    symlink(outside.path(), project.path().join(".inifra")).expect("create config symlink");

    let output = run_cli(&[
        "list",
        "--path",
        project.path().to_str().expect("UTF-8 project path"),
    ]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular directory"));
}

#[test]
fn generate_writes_template_to_the_requested_directory() {
    let destination = tempdir().expect("create temporary directory");
    let output = run_cli(&[
        "generate",
        "editorconfig",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Wrote 1 file"));
    assert_eq!(
        std::fs::read_to_string(destination.path().join(".editorconfig")).expect("read output"),
        "root = true\n\n[*]\ncharset = utf-8\nend_of_line = lf\ninsert_final_newline = true\n"
    );
}

#[test]
fn generate_dry_run_does_not_create_the_destination() {
    let parent = tempdir().expect("create temporary directory");
    let destination = parent.path().join("new-project");
    let output = run_cli(&[
        "generate",
        "changelog",
        "--path",
        destination.to_str().expect("UTF-8 path"),
        "--dry-run",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("create"));
    assert!(!destination.exists());
}

#[test]
fn generate_creates_a_missing_destination() {
    let parent = tempdir().expect("create temporary directory");
    let destination = parent.path().join("new-project");
    let output = run_cli(&[
        "generate",
        "editorconfig",
        "--path",
        destination.to_str().expect("UTF-8 path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(destination.join(".editorconfig")).expect("read output"),
        "root = true\n\n[*]\ncharset = utf-8\nend_of_line = lf\ninsert_final_newline = true\n"
    );
}

#[test]
fn changelog_template_avoids_assuming_a_versioning_policy() {
    let destination = tempdir().expect("create temporary directory");
    let output = run_cli(&[
        "generate",
        "changelog",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let changelog = std::fs::read_to_string(destination.path().join("CHANGELOG.md"))
        .expect("read generated changelog");
    assert!(changelog.contains("follows the [Keep a Changelog]"));
    assert!(changelog.contains("Use [Semantic Versioning]"));
    assert!(changelog.contains("this project's release model."));
    assert!(!changelog.contains("this project adheres to [Semantic Versioning]"));
    assert!(changelog.contains("## [Unreleased]"));
}

#[test]
fn init_generates_files_from_multiple_templates() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "init",
            "--template",
            "changelog",
            "--template",
            "editorconfig",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(destination.path().join(".editorconfig").is_file());
    assert!(destination.path().join("CHANGELOG.md").is_file());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Wrote 2 files"));
}

#[test]
fn init_detects_conflicting_outputs_before_writing_any_files() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "custom-editorconfig",
        "Custom editor settings",
        ".editorconfig",
        "custom\n",
    );
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "init",
            "--template",
            "editorconfig",
            "--template",
            "custom-editorconfig",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicting output paths"));
    assert!(!destination.path().join(".editorconfig").exists());
}

#[test]
fn init_requires_explicit_selection_without_a_terminal() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "init",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("provide --template ID or --preset NAME"),
        "unexpected error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        std::fs::read_dir(destination.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn saved_presets_can_be_listed_and_applied_with_cli_overrides() {
    let config = tempdir().expect("create isolated config directory");

    let saved = run_cli_with_config(
        &[
            "preset",
            "save",
            "starter",
            "--description",
            "A starter project",
            "--template",
            "readme",
            "--template",
            "editorconfig",
            "--set",
            "project_name=Preset name",
        ],
        config.path(),
    );
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );

    let listing = run_cli_with_config(&["preset", "list"], config.path());
    assert!(listing.status.success());
    assert!(String::from_utf8_lossy(&listing.stdout).contains("starter"));

    let destination = tempdir().expect("create destination");
    let initialized = run_cli_with_config(
        &[
            "init",
            "--preset",
            "starter",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
            "--set",
            "project_name=Override name",
        ],
        config.path(),
    );
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    assert!(destination.path().join(".editorconfig").is_file());
    let readme = std::fs::read_to_string(destination.path().join("README.md")).unwrap();
    assert!(readme.starts_with("# Override name\n"));
}

#[test]
fn generate_skips_existing_files_by_default() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".editorconfig");
    std::fs::write(&output_path, "keep this\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "editorconfig",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("skip (already exists)"));
    assert_eq!(
        std::fs::read_to_string(output_path).expect("read existing file"),
        "keep this\n"
    );
}

#[test]
fn generate_overwrites_only_when_requested() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".editorconfig");
    std::fs::write(&output_path, "old content\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "editorconfig",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
        "--overwrite",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("replace"));
    assert_eq!(
        std::fs::read_to_string(output_path).expect("read output"),
        "root = true\n\n[*]\ncharset = utf-8\nend_of_line = lf\ninsert_final_newline = true\n"
    );
}

#[test]
fn generate_dry_run_previews_replacement_without_changing_the_file() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".editorconfig");
    std::fs::write(&output_path, "old content\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "editorconfig",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
        "--overwrite",
        "--dry-run",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("replace"));
    assert_eq!(
        std::fs::read_to_string(output_path).expect("read existing file"),
        "old content\n"
    );
}

#[test]
fn generate_reports_unknown_template_ids_without_creating_the_destination() {
    let parent = tempdir().expect("create temporary directory");
    let destination = parent.path().join("new-project");
    let output = run_cli(&[
        "generate",
        "not-a-template",
        "--path",
        destination.to_str().expect("UTF-8 path"),
    ]);

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("unknown template"));
    assert!(error.contains("editorconfig"));
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn generate_refuses_to_replace_a_symlink() {
    use std::os::unix::fs::symlink;

    let destination = tempdir().expect("create destination");
    let outside = tempdir().expect("create outside directory");
    let outside_file = outside.path().join("outside.txt");
    std::fs::write(&outside_file, "keep this\n").expect("write outside file");
    symlink(&outside_file, destination.path().join(".editorconfig")).expect("create symlink");

    let output = run_cli(&[
        "generate",
        "editorconfig",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
        "--overwrite",
    ]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("symlink"));
    assert_eq!(
        std::fs::read_to_string(outside_file).expect("read outside file"),
        "keep this\n"
    );
}
