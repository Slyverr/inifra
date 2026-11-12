use super::*;

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
