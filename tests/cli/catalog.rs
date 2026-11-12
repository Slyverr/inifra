use super::*;

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
fn list_filter_searches_template_fields_case_insensitively() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("ops-guide");
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"ops-guide\"\ndescription = \"Team operations handbook\"\ncategory = \"Operations\"\n\n[[files]]\nsource = \"source.md\"\noutput = \"docs/on-call.md\"\n",
    )
    .expect("write local manifest");
    std::fs::write(pack.join("source.md"), "On-call procedures\n").expect("write template source");

    let output = run_cli_with_config(&["list", "--filter", "HANDBOOK on-call"], config.path());

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8_lossy(&output.stdout);
    assert!(listing.contains("ops-guide"));
    assert!(listing.contains("docs/on-call.md"));
    assert!(!listing.contains("agents"));
}

#[test]
fn list_filter_reports_when_nothing_matches() {
    let output = run_cli(&["list", "--filter", "no-such-template"]);

    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("No templates match \"no-such-template\".")
    );
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
fn validate_rejects_non_portable_template_paths_on_every_platform() {
    for (id, output_path) in [
        ("backslash-path", r"nested\..\outside.md"),
        ("drive-path", "C:/outside.md"),
        ("alternate-stream", "README.md:metadata"),
    ] {
        let config = tempdir().expect("create isolated config directory");
        write_local_pack(config.path(), id, "Path validation", output_path, "Notes\n");
        let manifest_path = templates_root(config.path()).join(id).join("template.toml");
        let manifest = std::fs::read_to_string(manifest_path).expect("read generated manifest");

        let output = run_cli_with_config(&["validate", id], config.path());

        assert!(
            !output.status.success(),
            "accepted path {output_path:?} from manifest {manifest:?}; stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("forward slashes"),
            "unexpected error for {output_path:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn list_rejects_terminal_control_characters_in_template_metadata() {
    let config = tempdir().expect("create isolated config directory");
    let pack = templates_root(config.path()).join("unsafe-description");
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"unsafe-description\"\ndescription = \"Safe text\\u001b[2J\"\n\n[[files]]\nsource = \"source.txt\"\noutput = \"NOTES.md\"\n",
    )
    .expect("write manifest with terminal control character");
    std::fs::write(pack.join("source.txt"), "Notes\n").expect("write template source");

    let output = run_cli_with_config(&["list"], config.path());

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("description must not contain control characters"));
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
}
