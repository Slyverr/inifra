use super::*;

#[cfg(unix)]
#[test]
fn generate_edit_opens_text_before_writing() {
    use std::os::unix::fs::PermissionsExt;

    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    let editor = config.path().join("fake-editor");
    std::fs::write(&editor, "#!/bin/sh\nprintf '# Edited\\n' > \"$1\"\n")
        .expect("write fake editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args([
            "generate",
            "readme",
            "--path",
            project.path().to_str().expect("project path is UTF-8"),
            "--set",
            "project_name=Example",
            "--edit",
        ])
        .env("VISUAL", &editor);
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run Inifra");

    assert!(output.status.success());
    assert_eq!(
        std::fs::read(project.path().join("README.md")).expect("read generated README"),
        b"# Edited\n"
    );
}

#[cfg(unix)]
#[test]
fn edit_copies_builtin_template_to_user_config_before_opening() {
    use std::os::unix::fs::PermissionsExt;

    let config = tempdir().expect("create isolated config directory");
    let editor = config.path().join("fake-editor");
    std::fs::write(
        &editor,
        "#!/bin/sh\nprintf '\\n<!-- customized -->\\n' >> \"$2\"\n",
    )
    .expect("write fake editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.args(["edit", "readme"]).env("VISUAL", &editor);
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run Inifra");
    let template_source = config.path().join("inifra/templates/readme/README.md.j2");

    assert!(output.status.success());
    assert!(template_source.is_file());
    assert!(
        std::fs::read_to_string(template_source)
            .expect("read edited local template")
            .contains("<!-- customized -->")
    );
}

#[cfg(unix)]
#[test]
fn edit_opens_the_project_local_template_when_given_a_project_path() {
    use std::os::unix::fs::PermissionsExt;

    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    write_project_pack(
        project.path(),
        "team-guide",
        "Team guide",
        "TEAM.md",
        "team\n",
    );
    let editor = config.path().join("fake-editor");
    std::fs::write(&editor, "#!/bin/sh\nprintf 'edited\\n' >> \"$2\"\n")
        .expect("write fake editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args([
            "edit",
            "team-guide",
            "--path",
            project.path().to_str().expect("UTF-8 project path"),
        ])
        .env("VISUAL", &editor);
    set_config_directory(&mut command, config.path());
    let output = command.output().expect("run project template editor");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(
            project
                .path()
                .join(".inifra/templates/team-guide/source.txt")
        )
        .expect("read edited project template"),
        "team\nedited\n"
    );
    assert!(!templates_root(config.path()).exists());
}

#[test]
fn edit_unknown_template_requires_an_interactive_terminal() {
    let output = run_cli(&["edit", "new-template"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("run `inifra edit new-template` in a terminal")
    );
}
