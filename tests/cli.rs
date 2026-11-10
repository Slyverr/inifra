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

fn set_config_directory(command: &mut Command, config: &std::path::Path) {
    #[cfg(target_os = "windows")]
    command.env("APPDATA", config);
    #[cfg(target_os = "macos")]
    command.env("HOME", config);
    #[cfg(all(unix, not(target_os = "macos")))]
    command.env("XDG_CONFIG_HOME", config);
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
    #[cfg(target_os = "macos")]
    let root = config.join("Library/Application Support/inifra/templates");
    #[cfg(not(target_os = "macos"))]
    let root = config.join("inifra/templates");
    root
}

#[test]
fn help_lists_available_commands() {
    let output = run_cli(&["--help"]);

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("list"));
    assert!(help.contains("generate"));
    assert!(help.contains("init"));
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
    assert!(listing.contains("editorconfig"));
    assert!(listing.contains(".editorconfig"));
    assert!(listing.contains("gitignore-rust"));
    assert!(listing.contains(".gitignore"));
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
    assert!(listing.contains("[local]"));
    assert!(listing.contains("NOTES.md"));
}

#[test]
fn local_template_with_a_builtin_id_overrides_the_builtin() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "gitignore-rust",
        "Custom ignore rules",
        ".gitignore",
        "custom\n",
    );
    let destination = tempdir().expect("create destination");

    let listing = run_cli_with_config(&["list"], config.path());
    assert!(listing.status.success());
    let listing = String::from_utf8_lossy(&listing.stdout);
    assert!(listing.contains("Custom ignore rules [local]"));
    assert!(!listing.contains("Basic Rust gitignore [built-in]"));

    let output = run_cli_with_config(
        &[
            "generate",
            "gitignore-rust",
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
        std::fs::read_to_string(destination.path().join(".gitignore")).unwrap(),
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
            "gitignore-rust",
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
    assert!(destination.path().join(".gitignore").is_file());
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
    let output = run_cli_in(&["list"], destination.path());

    assert!(output.status.success());
    assert!(!destination.path().join("CHANGELOG.md").exists());
}

#[test]
fn generate_writes_template_to_the_requested_directory() {
    let destination = tempdir().expect("create temporary directory");
    let output = run_cli(&[
        "generate",
        "gitignore-rust",
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
        std::fs::read_to_string(destination.path().join(".gitignore")).expect("read output"),
        "/target/\n"
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
    assert!(destination.join(".editorconfig").is_file());
}

#[test]
fn init_generates_files_from_multiple_templates() {
    let config = tempdir().expect("create isolated config directory");
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "init",
            "--template",
            "editorconfig",
            "--template",
            "gitignore-rust",
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
    assert!(destination.path().join(".gitignore").is_file());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Wrote 2 files"));
}

#[test]
fn init_detects_conflicting_outputs_before_writing_any_files() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "custom-ignore",
        "Custom ignore rules",
        ".gitignore",
        "custom\n",
    );
    let destination = tempdir().expect("create destination");

    let output = run_cli_with_config(
        &[
            "init",
            "--template",
            "gitignore-rust",
            "--template",
            "custom-ignore",
            "--path",
            destination.path().to_str().expect("UTF-8 path"),
        ],
        config.path(),
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicting output paths"));
    assert!(!destination.path().join(".gitignore").exists());
}

#[test]
fn init_requires_an_explicit_template_until_interactive_selection_exists() {
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("select at least one template"));
    assert!(
        std::fs::read_dir(destination.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn generate_skips_existing_files_by_default() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".gitignore");
    std::fs::write(&output_path, "keep this\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
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
    let output_path = destination.path().join(".gitignore");
    std::fs::write(&output_path, "old content\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
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
        "/target/\n"
    );
}

#[test]
fn generate_dry_run_previews_replacement_without_changing_the_file() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".gitignore");
    std::fs::write(&output_path, "old content\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
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
    assert!(error.contains("gitignore-rust"));
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
    symlink(&outside_file, destination.path().join(".gitignore")).expect("create symlink");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
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
