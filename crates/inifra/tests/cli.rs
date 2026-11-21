use std::{
    fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_CONFIG: AtomicUsize = AtomicUsize::new(0);

fn inifra() -> Command {
    let config_dir = std::env::temp_dir().join(format!(
        "inifra-cli-test-{}-{}",
        std::process::id(),
        NEXT_CONFIG.fetch_add(1, Ordering::Relaxed)
    ));
    inifra_with_config(&config_dir)
}

fn inifra_with_config(config_dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.env("INIFRA_CONFIG_DIR", config_dir);
    command.env("INIFRA_CACHE_DIR", config_dir.join("cache"));
    command
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_NAME", "Inifra test")
        .env("GIT_AUTHOR_EMAIL", "inifra-test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Inifra test")
        .env("GIT_COMMITTER_EMAIL", "inifra-test@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn git_catalog() -> tempfile::TempDir {
    let repository = tempfile::tempdir().unwrap();
    git(repository.path(), &["init", "--quiet"]);
    let pack = repository.path().join("project-files/guide");
    fs::create_dir_all(&pack).unwrap();
    fs::write(
        pack.join("template.toml"),
        "schema_version = 2\ndescription = 'Team guide'\n",
    )
    .unwrap();
    fs::write(pack.join("GUIDE.md.j2"), "Version one\n").unwrap();
    git(repository.path(), &["add", "."]);
    git(
        repository.path(),
        &["commit", "--quiet", "-m", "catalog v1"],
    );
    git(repository.path(), &["branch", "-M", "main"]);
    repository
}

fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let path = path
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

#[test]
fn help_lists_catalog_commands() {
    let output = inifra().arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(
        !help.contains("\u{1b}["),
        "redirected help should be plain text"
    );
    for command in [
        "init",
        "list",
        "inspect",
        "validate",
        "generate",
        "source",
        "assistant",
        "ask",
    ] {
        assert!(help.contains(command));
    }
}

#[test]
fn ask_requires_a_terminal_before_reading_assistant_configuration() {
    let config_dir = tempfile::tempdir().unwrap();
    let output = inifra_with_config(config_dir.path())
        .arg("ask")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("requires an interactive terminal"),
        "{error}"
    );
    assert!(!config_dir.path().join("assistant.toml").exists());
    assert!(!config_dir.path().join("credentials.env").exists());
}

#[test]
fn init_requires_a_terminal_and_does_not_touch_the_target() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("new-project");
    let output = inifra()
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
    let output = inifra()
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
fn project_local_pack_overlays_bundled_pack_for_inspection_and_generation() {
    let project = tempfile::tempdir().unwrap();
    let pack = project
        .path()
        .join(".inifra/templates/project-files/readme");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 2\ndescription = 'Team README'\n\n[local_inputs.project_name]\nprompt = 'Project name'\nrequired = true\n",
    )
    .unwrap();
    std::fs::write(
        pack.join("README.md.j2"),
        "# {{ project_name }}\n\nTeam edition.\n",
    )
    .unwrap();

    let inspected = inifra()
        .current_dir(project.path())
        .args(["inspect", "readme"])
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    assert!(String::from_utf8_lossy(&inspected.stdout).contains("Team README"));

    let target = project.path().join("generated");
    let generated = inifra()
        .current_dir(project.path())
        .args([
            "generate",
            "readme",
            "--target",
            target.to_str().unwrap(),
            "--set",
            "project_name=Example",
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(target.join("README.md")).unwrap(),
        "# Example\n\nTeam edition.\n"
    );
}

#[test]
fn source_commands_pin_update_cache_and_use_snapshots_offline() {
    let project = tempfile::tempdir().unwrap();
    let repository = git_catalog();
    let config = tempfile::tempdir().unwrap();
    let url = file_url(repository.path());
    let index = project.path().join(".inifra/templates/project-docs/index");
    fs::create_dir_all(&index).unwrap();
    fs::write(
        index.join("template.toml"),
        "schema_version = 2\ndescription = 'Project documentation index'\n\n[references.guide]\ntemplate = 'team/guide'\nsource = 'GUIDE.md.j2'\n",
    )
    .unwrap();
    fs::write(
        index.join("README.md.j2"),
        "Read the [guide]({{ references.guide }}).\n",
    )
    .unwrap();

    let added = inifra_with_config(config.path())
        .current_dir(project.path())
        .args(["source", "add", "team", &url, "--ref", "refs/heads/main"])
        .env("GIT_DIR", config.path().join("not-a-repository"))
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let registry_path = project.path().join(".inifra/sources.toml");
    let mut registry = inifra_core::source_registry::SourceRegistry::load(&registry_path).unwrap();
    let first_revision = registry.sources["team"].revision.clone();
    let first_snapshot = config
        .path()
        .join("cache/sources/team")
        .join(&first_revision);
    assert!(first_snapshot.is_dir());

    fs::remove_dir_all(&first_snapshot).unwrap();
    let synced = inifra_with_config(config.path())
        .current_dir(project.path())
        .args(["source", "sync", "team"])
        .output()
        .unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(first_snapshot.is_dir());

    fs::write(
        repository.path().join("project-files/guide/GUIDE.md.j2"),
        "Version two\n",
    )
    .unwrap();
    git(repository.path(), &["add", "."]);
    git(
        repository.path(),
        &["commit", "--quiet", "-m", "catalog v2"],
    );
    let updated = inifra_with_config(config.path())
        .current_dir(project.path())
        .args(["source", "update", "team"])
        .output()
        .unwrap();
    assert!(
        updated.status.success(),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );
    registry = inifra_core::source_registry::SourceRegistry::load(&registry_path).unwrap();
    let second_revision = registry.sources["team"].revision.clone();
    assert_ne!(first_revision, second_revision);

    fs::rename(repository.path(), project.path().join("source-offline")).unwrap();
    let completions = inifra_with_config(config.path())
        .current_dir(project.path())
        .args(["--", "inifra", "generate", "team/"])
        .env("INIFRA_COMPLETE", "fish")
        .output()
        .unwrap();
    assert!(
        completions.status.success(),
        "{}",
        String::from_utf8_lossy(&completions.stderr)
    );
    assert!(
        String::from_utf8_lossy(&completions.stdout).contains("team/guide"),
        "{}",
        String::from_utf8_lossy(&completions.stdout)
    );

    let inspected = inifra_with_config(config.path())
        .current_dir(project.path())
        .args(["inspect", "team/guide"])
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let target = project.path().join("generated");
    let generated = inifra_with_config(config.path())
        .current_dir(project.path())
        .args([
            "generate",
            "index",
            "team/guide",
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert_eq!(
        fs::read_to_string(target.join("GUIDE.md")).unwrap(),
        "Version two\n"
    );
    assert_eq!(
        fs::read_to_string(target.join("README.md")).unwrap(),
        "Read the [guide](GUIDE.md).\n"
    );

    let removed = inifra_with_config(config.path())
        .current_dir(project.path())
        .args(["source", "remove", "team"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(
        config
            .path()
            .join("cache/sources/team")
            .join(second_revision)
            .is_dir()
    );
    assert!(
        !inifra_with_config(config.path())
            .current_dir(project.path())
            .args(["inspect", "team/guide"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn catalog_commands_explain_how_to_restore_a_missing_source_pin() {
    let project = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join(".inifra")).unwrap();
    let mut registry = inifra_core::source_registry::SourceRegistry::default();
    registry
        .insert(
            "team",
            inifra_core::source_registry::GitSource {
                url: "https://example.com/team.git".into(),
                reference: "refs/heads/main".into(),
                revision: "0123456789abcdef0123456789abcdef01234567".into(),
            },
        )
        .unwrap();
    registry
        .save(&project.path().join(".inifra/sources.toml"))
        .unwrap();

    let output = inifra_with_config(config.path())
        .current_dir(project.path())
        .arg("list")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("team"), "{error}");
    assert!(error.contains("inifra source sync team"), "{error}");
}

#[test]
fn inspect_shows_template_inputs_and_outputs() {
    let output = inifra().args(["inspect", "readme"]).output().unwrap();
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
    let output = inifra()
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
    let output = inifra()
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
        inifra()
            .args([
                "generate",
                "editorconfig",
                "--target",
                target.path().to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };

    let created = run();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(String::from_utf8_lossy(&created.stdout).contains("created  .editorconfig"));
    let path = target.path().join(".editorconfig");
    assert!(path.is_file());
    std::fs::write(&path, "custom").unwrap();
    let skipped = run();
    assert!(
        skipped.status.success(),
        "{}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    assert!(String::from_utf8_lossy(&skipped.stdout).contains("already exists  .editorconfig"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "custom");
}

#[test]
fn generates_readme_with_required_input_and_no_optional_description() {
    let target = tempfile::tempdir().unwrap();
    let output = inifra()
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
    let output = inifra()
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
    let output = inifra()
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
    let output = inifra()
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
    let output = inifra()
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
    let output = inifra()
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
    let output = inifra()
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
    let output = inifra()
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
    let list = inifra()
        .args(["--catalog", catalog.to_str().unwrap(), "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).contains("editorconfig"));
    assert!(String::from_utf8_lossy(&list.stdout).contains("Project files\n"));
    let dry_run = inifra()
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

    let compact = inifra()
        .args(["--catalog", catalog.to_str().unwrap(), "list"])
        .output()
        .unwrap();
    assert!(compact.status.success());
    let text = String::from_utf8(compact.stdout).unwrap();
    assert!(
        !text.contains("\u{1b}["),
        "redirected output should stay plain"
    );
    assert!(text.contains("bug-report.yml +2 other files, 1 directory"));
    assert!(!text.contains("feature-request.yml"));

    let full = inifra()
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
        ".github/ISSUE_TEMPLATE/{bug-report.yml, feature-request.yml, question.yml}",
        "docs/",
    ] {
        assert!(text.contains(path), "missing {path} in:\n{text}");
    }
    assert!(!text.contains("Outputs:"));
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
    let dry_run = inifra()
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
    let dry_run_output = String::from_utf8_lossy(&dry_run.stdout);
    assert!(
        dry_run_output
            .lines()
            .any(|line| line.contains("create directory") && line.contains("code/"))
    );
    assert!(dry_run_output.contains("No files were changed."));
    assert!(!target.exists());

    let inspect = inifra()
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

    let generate = inifra()
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

    let existing_directory_dry_run = inifra()
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
        existing_directory_dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&existing_directory_dry_run.stderr)
    );
    let dry_run_output = String::from_utf8_lossy(&existing_directory_dry_run.stdout);
    assert!(
        dry_run_output
            .lines()
            .any(|line| line.contains("skip (already exists)") && line.contains("code/"))
    );
}

fn write_catalog_file(root: &std::path::Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn default_catalog_adds_user_packs_and_replaces_matching_ids() {
    let config = tempfile::tempdir().unwrap();
    let templates = config.path().join("templates");
    write_catalog_file(
        &templates,
        "custom/notes/template.toml",
        "schema_version = 2\ndescription = 'Personal notes'\n",
    );
    write_catalog_file(&templates, "custom/notes/NOTES.md", "Notes\n");
    write_catalog_file(
        &templates,
        "custom/editorconfig/template.toml",
        "schema_version = 2\ndescription = 'My editor defaults'\n",
    );
    write_catalog_file(
        &templates,
        "custom/editorconfig/.editorconfig",
        "root = true\n",
    );

    let output = inifra_with_config(config.path())
        .args(["list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8(output.stdout).unwrap();
    assert!(listing.contains("Personal notes"), "{listing}");
    assert!(listing.contains("My editor defaults"), "{listing}");
    assert!(!listing.contains("Portable text encoding and newline defaults"));

    let target = tempfile::tempdir().unwrap();
    let output = inifra_with_config(config.path())
        .args([
            "generate",
            "editorconfig",
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
    assert_eq!(
        std::fs::read_to_string(target.path().join(".editorconfig")).unwrap(),
        "root = true\n"
    );
}

#[test]
fn explicit_catalog_remains_exclusive_of_user_packs() {
    let config = tempfile::tempdir().unwrap();
    write_catalog_file(
        &config.path().join("templates"),
        "custom/notes/template.toml",
        "schema_version = 2\ndescription = 'Personal notes'\n",
    );
    write_catalog_file(
        &config.path().join("templates"),
        "custom/notes/NOTES.md",
        "Notes\n",
    );
    let explicit = tempfile::tempdir().unwrap();
    write_catalog_file(
        explicit.path(),
        "custom/only/template.toml",
        "schema_version = 2\ndescription = 'Explicit catalog only'\n",
    );
    write_catalog_file(explicit.path(), "custom/only/ONLY.md", "Only\n");

    let output = inifra_with_config(config.path())
        .args(["--catalog", explicit.path().to_str().unwrap(), "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8(output.stdout).unwrap();
    assert!(listing.contains("Explicit catalog only"), "{listing}");
    assert!(!listing.contains("Personal notes"), "{listing}");
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
    let output = inifra()
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
    let output = inifra()
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
    let output = inifra()
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
