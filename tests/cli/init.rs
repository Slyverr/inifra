use super::*;

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
