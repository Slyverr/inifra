use super::*;

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
