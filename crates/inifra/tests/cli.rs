use std::process::Command;

#[test]
fn help_lists_catalog_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for command in ["list", "inspect", "validate", "generate"] {
        assert!(help.contains(command));
    }
}

#[test]
fn bundled_catalog_is_available_outside_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .current_dir(dir.path())
        .arg("list")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("editorconfig"));
}

#[test]
fn generates_and_preserves_existing_files_without_overwrite() {
    let target = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_inifra"))
            .args([
                "generate",
                "editorconfig",
                "--target",
                target.path().to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let path = target.path().join(".editorconfig");
    assert!(path.is_file());
    std::fs::write(&path, "custom").unwrap();
    assert!(run().status.success());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "custom");
}

#[test]
fn lists_catalog_and_dry_run_does_not_create_target() {
    let catalog = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates");
    let target = std::env::temp_dir().join(format!("inifra-cli-dry-run-{}", std::process::id()));
    let list = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--catalog", catalog.to_str().unwrap(), "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).contains("editorconfig"));
    let dry_run = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "generate",
            "editorconfig",
            "--target",
            target.to_str().unwrap(),
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!target.exists());
}
