use super::*;

#[test]
fn remove_deletes_only_the_requested_user_template_pack() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    write_local_pack(
        config.path(),
        "custom-notes",
        "Personal notes",
        "NOTES.md",
        "notes\n",
    );
    write_local_pack(
        config.path(),
        "other-notes",
        "Other notes",
        "OTHER.md",
        "other\n",
    );
    let generated_file = project.path().join("NOTES.md");
    std::fs::write(&generated_file, "keep this\n").expect("write generated project file");

    let output = run_cli_with_config(&["remove", "custom-notes"], config.path());

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!templates_root(config.path()).join("custom-notes").exists());
    assert!(templates_root(config.path()).join("other-notes").is_dir());
    assert_eq!(
        std::fs::read_to_string(generated_file).expect("read generated project file"),
        "keep this\n"
    );
}

#[test]
fn removing_a_user_override_restores_the_builtin_template() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "readme",
        "Custom README",
        "README.md",
        "custom\n",
    );

    let output = run_cli_with_config(&["remove", "readme"], config.path());
    let listing = run_cli_with_config(&["list"], config.path());
    let listing = String::from_utf8_lossy(&listing.stdout);

    assert!(output.status.success());
    assert!(listing.contains("readme"));
    assert!(listing.contains("[built-in]"));
    assert!(!listing.contains("[user-local]"));
}

#[test]
fn project_removal_reveals_the_user_template_without_deleting_it() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    write_local_pack(
        config.path(),
        "team-notes",
        "Personal notes",
        "NOTES.md",
        "personal\n",
    );
    write_project_pack(
        project.path(),
        "team-notes",
        "Team notes",
        "NOTES.md",
        "team\n",
    );

    let output = run_cli_with_config(
        &[
            "remove",
            "team-notes",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ],
        config.path(),
    );
    let listing = run_cli_with_config(
        &[
            "list",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ],
        config.path(),
    );

    assert!(output.status.success());
    assert!(!project.path().join(".inifra/templates/team-notes").exists());
    assert!(templates_root(config.path()).join("team-notes").is_dir());
    assert!(String::from_utf8_lossy(&listing.stdout).contains("[user-local]"));
}

#[test]
fn project_removal_preserves_shared_inputs_and_source_declarations() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    write_project_inputs(project.path(), "team_name", "Platform team");
    let inputs_path = project.path().join(".inifra/inputs.toml");
    let inputs = std::fs::read_to_string(&inputs_path).expect("read project input definitions");
    let source_registry = "schema_version = 1\n\n[sources.team]\nurl = \"https://example.com/team/templates.git\"\nrevision = \"0123456789012345678901234567890123456789\"\n";
    let source_registry_path = project.path().join(".inifra/sources.toml");
    std::fs::write(&source_registry_path, source_registry).expect("write project source registry");
    write_project_pack(
        project.path(),
        "team-notes",
        "Team notes",
        "NOTES.md",
        "team\n",
    );

    let output = run_cli_with_config(
        &[
            "remove",
            "team-notes",
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
    assert!(!project.path().join(".inifra/templates/team-notes").exists());
    assert_eq!(
        std::fs::read_to_string(inputs_path).expect("read project input definitions"),
        inputs
    );
    assert_eq!(
        std::fs::read_to_string(source_registry_path).expect("read project source registry"),
        source_registry
    );
}

#[test]
fn remove_does_not_delete_a_builtin_without_a_local_pack() {
    let config = tempdir().expect("create isolated config directory");
    let output = run_cli_with_config(&["remove", "readme"], config.path());

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no user-local template pack"));
    let listing = run_cli_with_config(&["list"], config.path());
    assert!(String::from_utf8_lossy(&listing.stdout).contains("[built-in]"));
}

#[test]
fn remove_rejects_remote_template_ids() {
    let config = tempdir().expect("create isolated config directory");
    let output = run_cli_with_config(&["remove", "team/notes"], config.path());

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("remote template"));
}

#[cfg(unix)]
#[test]
fn remove_rejects_a_symlinked_template_pack_without_touching_its_target() {
    use std::os::unix::fs::symlink;

    let config = tempdir().expect("create isolated config directory");
    let outside = tempdir().expect("create outside directory");
    std::fs::write(outside.path().join("keep.txt"), "keep this\n").expect("write outside file");
    let template_root = templates_root(config.path());
    std::fs::create_dir_all(&template_root).expect("create local templates directory");
    symlink(outside.path(), template_root.join("custom-notes"))
        .expect("create template pack symlink");

    let output = run_cli_with_config(&["remove", "custom-notes"], config.path());

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular directory"));
    assert_eq!(
        std::fs::read_to_string(outside.path().join("keep.txt")).expect("read outside file"),
        "keep this\n"
    );
}
