use super::*;

#[test]
fn list_is_independent_of_the_current_directory() {
    let destination = tempdir().expect("create temporary directory");
    write_project_pack(
        destination.path(),
        "project-only",
        "Project-only template",
        "PROJECT.md",
        "project\n",
    );
    let output = run_cli_in(&["list"], destination.path());

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("project-only"));
    assert!(!destination.path().join("CHANGELOG.md").exists());
}

#[test]
fn list_includes_project_templates_only_when_given_a_project_path() {
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "team-guide",
        "Team guide",
        "TEAM.md",
        "team\n",
    );

    let output = run_cli(&[
        "list",
        "--path",
        project.path().to_str().expect("UTF-8 project path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8_lossy(&output.stdout);
    assert!(listing.contains("team-guide"));
    assert!(listing.contains("[project-local]"));
}

#[test]
fn project_templates_override_user_templates_for_generation() {
    let config = tempdir().expect("create isolated config directory");
    write_local_pack(
        config.path(),
        "editorconfig",
        "User editor settings",
        ".editorconfig",
        "user\n",
    );
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "editorconfig",
        "Project editor settings",
        ".editorconfig",
        "project\n",
    );

    let output = run_cli_with_config(
        &[
            "generate",
            "editorconfig",
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
    assert_eq!(
        std::fs::read_to_string(project.path().join(".editorconfig")).expect("read generated file"),
        "project\n"
    );
}

#[test]
fn validate_includes_project_templates_when_given_a_project_path() {
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "team-guide",
        "Team guide",
        "TEAM.md",
        "team\n",
    );

    let output = run_cli(&[
        "validate",
        "--path",
        project.path().to_str().expect("UTF-8 project path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("valid team-guide"));
}

#[test]
fn project_templates_use_project_shared_input_definitions() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    let user_config = config.path().join("inifra");
    std::fs::create_dir_all(&user_config).expect("create user Inifra config");
    std::fs::write(
        user_config.join("inputs.toml"),
        "schema_version = 1\n\n[inputs.team_name]\nprompt = \"Team name\"\ndefault = \"Personal team\"\n",
    )
    .expect("write user input definitions");
    write_project_inputs(project.path(), "team_name", "Platform team");
    let pack = project.path().join(".inifra/templates/team-guide");
    std::fs::create_dir_all(&pack).expect("create project template pack");
    std::fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"team-guide\"\ndescription = \"Team guide\"\ninputs = [\"team_name\"]\n\n[[files]]\nsource = \"TEAM.md.j2\"\noutput = \"TEAM.md\"\n",
    )
    .expect("write project template manifest");
    std::fs::write(pack.join("TEAM.md.j2"), "# {{ team_name }}\n")
        .expect("write project template source");

    let output = run_cli_with_config(
        &[
            "generate",
            "team-guide",
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
    assert_eq!(
        std::fs::read_to_string(project.path().join("TEAM.md")).expect("read generated file"),
        "# Platform team\n"
    );
    let validation = run_cli_with_config(
        &[
            "validate",
            "team-guide",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ],
        config.path(),
    );
    assert!(
        validation.status.success(),
        "{}",
        String::from_utf8_lossy(&validation.stderr)
    );
}

#[test]
fn project_input_registry_is_ignored_for_templates_that_use_no_inputs() {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    let project_config = project.path().join(".inifra");
    std::fs::create_dir_all(&project_config).expect("create project Inifra config");
    std::fs::write(project_config.join("inputs.toml"), "malformed = [")
        .expect("write unrelated malformed input registry");

    let output = run_cli_with_config(
        &[
            "generate",
            "editorconfig",
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
    assert!(project.path().join(".editorconfig").is_file());
}

#[cfg(unix)]
#[test]
fn project_template_discovery_rejects_a_symlinked_config_directory() {
    use std::os::unix::fs::symlink;

    let project = tempdir().expect("create project directory");
    let outside = tempdir().expect("create outside directory");
    symlink(outside.path(), project.path().join(".inifra")).expect("create config symlink");

    let output = run_cli(&[
        "list",
        "--path",
        project.path().to_str().expect("UTF-8 project path"),
    ]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular directory"));
}
