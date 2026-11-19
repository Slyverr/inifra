use std::process::Command;

#[test]
fn help_lists_catalog_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for command in ["init", "list", "inspect", "validate", "generate"] {
        assert!(help.contains(command));
    }
}

#[test]
fn init_requires_a_terminal_and_does_not_touch_the_target() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("new-project");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["init", "--target", target.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("requires an interactive terminal"),
        "{error}"
    );
    assert!(error.contains("inifra generate"), "{error}");
    assert!(!target.exists());
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
fn inspect_shows_template_inputs_and_outputs() {
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["inspect", "readme"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("README.md"));
    assert!(text.contains("project_name (required)"));
    assert!(text.contains("project_description (optional)"));
}

#[test]
fn inspect_describes_an_output_alternative() {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    write_catalog_file(
        &catalog,
        "project/terms/template.toml",
        "schema_version = 2\ndescription = 'Terms'\n[[output_rules]]\nsource = 'TERMS.md'\nco_selected_output = { group = 'terms', path = 'TERMS-WEB.md' }\n",
    );
    write_catalog_file(&catalog, "project/terms/TERMS.md", "Terms\n");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--catalog", catalog.to_str().unwrap(), "inspect", "terms"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("TERMS.md"));
    assert!(text.contains("when paired (terms)  TERMS-WEB.md"));
}

#[test]
fn generate_uses_co_selected_output_alternatives() {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    for (id, alternative) in [
        ("license-mit", "LICENSE-MIT"),
        ("license-apache", "LICENSE-APACHE"),
    ] {
        write_catalog_file(
            &catalog,
            &format!("project-policies/{id}/template.toml"),
            &format!(
                "schema_version = 2\ndescription = 'License'\n[[output_rules]]\nsource = 'LICENSE.txt'\nco_selected_output = {{ group = 'licenses', path = '{alternative}' }}\n"
            ),
        );
        write_catalog_file(&catalog, &format!("project-policies/{id}/LICENSE.txt"), id);
    }
    let target = root.path().join("target");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "generate",
            "license-mit",
            "license-apache",
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(target.join("LICENSE-MIT")).unwrap(),
        "license-mit"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("LICENSE-APACHE")).unwrap(),
        "license-apache"
    );
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
fn generates_readme_with_required_input_and_no_optional_description() {
    let target = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "generate",
            "readme",
            "--set",
            "project_name=Example",
            "--target",
            target.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let readme = std::fs::read_to_string(target.path().join("README.md")).unwrap();
    assert_eq!(readme, "# Example\n");
    assert!(!readme.contains("CONTRIBUTING.md"));
    assert!(!readme.contains("LICENSE"));
}

#[test]
fn contributing_describes_only_the_selected_project_documentation() {
    let target = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "generate",
            "contributing",
            "architecture",
            "--set",
            "project_name=Example",
            "--target",
            target.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let contributing = std::fs::read_to_string(target.path().join("CONTRIBUTING.md")).unwrap();
    assert!(contributing.contains("| `docs/` | Project documentation |"));
    assert!(contributing.contains("[the architecture guide](docs/architecture.md)"));
    assert!(!contributing.contains("development guide"));
    assert!(!target.path().join("docs/development.md").exists());
}

#[test]
fn bundled_references_and_license_alternatives_follow_selected_packs() {
    let target = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "generate",
            "readme",
            "agents",
            "contributing",
            "architecture",
            "development",
            "security",
            "license-mit",
            "license-apache",
            "--target",
            target.path().to_str().unwrap(),
            "--set",
            "project_name=Example",
            "--set",
            "copyright_year=2026",
            "--set",
            "copyright_holder=Example Maintainers",
            "--set",
            "security_contact=security@example.org",
            "--set",
            "supported_versions=Current release",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let readme = std::fs::read_to_string(target.path().join("README.md")).unwrap();
    assert!(readme.contains("[CONTRIBUTING.md](CONTRIBUTING.md)"));
    assert!(readme.contains("[MIT License](LICENSE-MIT)"));
    assert!(readme.contains("[Apache License 2.0](LICENSE-APACHE)"));

    let agents = std::fs::read_to_string(target.path().join("AGENTS.md")).unwrap();
    assert!(agents.contains("[README.md](README.md)"));
    assert!(agents.contains("[CONTRIBUTING.md](CONTRIBUTING.md)"));

    let contributing = std::fs::read_to_string(target.path().join("CONTRIBUTING.md")).unwrap();
    assert!(contributing.contains("[the architecture guide](docs/architecture.md)"));
    assert!(contributing.contains("[the development guide](docs/development.md)"));
    assert!(contributing.contains("[the security policy](SECURITY.md)"));

    let copyright = "Copyright 2026 Example Maintainers";
    assert!(
        std::fs::read_to_string(target.path().join("LICENSE-MIT"))
            .unwrap()
            .contains(copyright)
    );
    let apache_license = std::fs::read_to_string(target.path().join("LICENSE-APACHE")).unwrap();
    assert!(apache_license.contains(copyright));
    assert!(
        apache_license
            .lines()
            .next()
            .is_some_and(|line| line.contains("Apache License"))
    );
    assert!(target.path().join("LICENSE-MIT").is_file());
    assert!(target.path().join("LICENSE-APACHE").is_file());
    assert!(!target.path().join("LICENSE").exists());
}

#[test]
fn bundled_license_keeps_its_default_path_when_selected_alone() {
    let target = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "generate",
            "readme",
            "license-mit",
            "--target",
            target.path().to_str().unwrap(),
            "--set",
            "project_name=Example",
            "--set",
            "copyright_year=2026",
            "--set",
            "copyright_holder=Example Maintainers",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let readme = std::fs::read_to_string(target.path().join("README.md")).unwrap();
    assert!(readme.contains("[MIT License](LICENSE)"));
    assert!(!readme.contains("Apache License"));
    assert!(target.path().join("LICENSE").is_file());
    assert!(!target.path().join("LICENSE-MIT").exists());
}

#[test]
fn missing_required_input_does_not_create_target() {
    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join("project");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["generate", "readme", "--target", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("project_name"));
    assert!(!target.exists());
}

#[test]
fn overwrite_must_be_explicit() {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join(".editorconfig");
    std::fs::write(&path, "custom").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "generate",
            "editorconfig",
            "--target",
            target.path().to_str().unwrap(),
            "--overwrite",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(std::fs::read_to_string(path).unwrap(), "custom");
}

#[test]
fn validate_rejects_invalid_template_syntax() {
    let root = tempfile::tempdir().unwrap();
    let pack = root.path().join("category/broken");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(root.path().join("inputs.toml"), "schema_version = 1\n").unwrap();
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 2\ndescription = 'Broken'\n",
    )
    .unwrap();
    std::fs::write(pack.join("README.md.j2"), "{% if %}").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--catalog", root.path().to_str().unwrap(), "validate"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("README.md.j2"));
}

#[test]
fn generation_failure_does_not_write_other_selected_outputs() {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    for (name, file, content) in [
        ("good", "good.txt", "ready"),
        ("broken", "broken.txt.j2", "{{ missing }}"),
    ] {
        let pack = catalog.join("project").join(name);
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(
            pack.join("template.toml"),
            "schema_version = 2\ndescription = 'Example'\n",
        )
        .unwrap();
        std::fs::write(pack.join(file), content).unwrap();
    }
    std::fs::write(catalog.join("inputs.toml"), "schema_version = 1\n").unwrap();
    let target = root.path().join("target");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "generate",
            "good",
            "broken",
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("broken.txt.j2"));
    assert!(!target.exists());
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
    assert!(String::from_utf8_lossy(&list.stdout).contains("Project files\n"));
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

#[test]
fn list_summarizes_outputs_and_files_shows_every_path() {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    write_catalog_file(
        &catalog,
        "github/github-issues/template.toml",
        "schema_version = 2\ndescription = 'Issue forms'\ndirectories = ['docs']\n",
    );
    for name in ["bug-report.yml", "feature-request.yml", "question.yml"] {
        write_catalog_file(
            &catalog,
            &format!("github/github-issues/.github/ISSUE_TEMPLATE/{name}"),
            "name: Example\n",
        );
    }

    let compact = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--catalog", catalog.to_str().unwrap(), "list"])
        .output()
        .unwrap();
    assert!(compact.status.success());
    let text = String::from_utf8(compact.stdout).unwrap();
    assert!(text.contains("bug-report.yml, +2 other files, 1 directory"));
    assert!(!text.contains("feature-request.yml"));

    let full = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--catalog", catalog.to_str().unwrap(), "list", "--files"])
        .output()
        .unwrap();
    assert!(
        full.status.success(),
        "{}",
        String::from_utf8_lossy(&full.stderr)
    );
    let text = String::from_utf8(full.stdout).unwrap().replace('\\', "/");
    for path in [
        ".github/ISSUE_TEMPLATE/bug-report.yml",
        ".github/ISSUE_TEMPLATE/feature-request.yml",
        ".github/ISSUE_TEMPLATE/question.yml",
        "docs/",
    ] {
        assert!(text.contains(path), "missing {path} in:\n{text}");
    }
}

#[test]
fn generates_a_directory_only_template_and_dry_run_is_read_only() {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    write_catalog_file(
        &catalog,
        "workspace/workspace/template.toml",
        "schema_version = 2\ndescription = 'Workspace structure'\ndirectories = ['code', 'drafts', 'research', 'worktrees']\n",
    );
    let target = root.path().join("target");
    let dry_run = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "generate",
            "workspace",
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
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("create: code/"));
    assert!(!target.exists());

    let inspect = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "inspect",
            "workspace",
        ])
        .output()
        .unwrap();
    assert!(inspect.status.success());
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("Directories:\n  code/"));

    let generate = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "generate",
            "workspace",
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        generate.status.success(),
        "{}",
        String::from_utf8_lossy(&generate.stderr)
    );
    for directory in ["code", "drafts", "research", "worktrees"] {
        assert!(target.join(directory).is_dir(), "{directory}");
    }
}

fn write_catalog_file(root: &std::path::Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn list_uses_category_display_name_override() {
    let root = tempfile::tempdir().unwrap();
    write_catalog_file(
        root.path(),
        "categories.toml",
        "schema_version = 1\n[categories.project-docs]\nname = 'Project documentation'\n",
    );
    write_catalog_file(
        root.path(),
        "project-docs/guide/template.toml",
        "schema_version = 2\ndescription = 'A guide'\n",
    );
    write_catalog_file(root.path(), "project-docs/guide/guide.md", "Guide\n");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--catalog", root.path().to_str().unwrap(), "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Project documentation\n"));
}

#[test]
fn generate_uses_root_category_and_pack_local_inputs() {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    write_catalog_file(
        &catalog,
        "inputs.toml",
        "schema_version = 1\n[inputs.name]\nprompt = 'Root name'\ndefault = 'root'\n[inputs.root_only]\nprompt = 'Root only'\ndefault = 'shared'\n",
    );
    write_catalog_file(
        &catalog,
        "project/inputs.toml",
        "schema_version = 1\n[inputs.name]\nprompt = 'Category name'\ndefault = 'category'\n",
    );
    write_catalog_file(
        &catalog,
        "project/guide/template.toml",
        "schema_version = 2\ndescription = 'A guide'\ninputs = ['name', 'root_only']\n[local_inputs.local_only]\nprompt = 'Local value'\ndefault = 'local'\n",
    );
    write_catalog_file(
        &catalog,
        "project/guide/guide.txt.j2",
        "{{ name }} {{ root_only }} {{ local_only }}",
    );
    let target = root.path().join("target");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "generate",
            "guide",
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(target.join("guide.txt")).unwrap(),
        "category shared local"
    );
}

#[test]
fn generate_rejects_conflicting_scoped_inputs_without_writing() {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    for (category, pack, default) in [("one", "first", "one"), ("two", "second", "two")] {
        write_catalog_file(
            &catalog,
            &format!("{category}/inputs.toml"),
            &format!("schema_version = 1\n[inputs.name]\nprompt = 'Name'\ndefault = '{default}'\n"),
        );
        write_catalog_file(
            &catalog,
            &format!("{category}/{pack}/template.toml"),
            "schema_version = 2\ndescription = 'Example'\ninputs = ['name']\n",
        );
        write_catalog_file(
            &catalog,
            &format!("{category}/{pack}/{pack}.txt.j2"),
            "{{ name }}",
        );
    }
    let target = root.path().join("target");
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args([
            "--catalog",
            catalog.to_str().unwrap(),
            "generate",
            "first",
            "second",
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicting definitions"));
    assert!(!target.exists());
}
