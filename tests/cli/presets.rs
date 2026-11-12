use super::*;

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
