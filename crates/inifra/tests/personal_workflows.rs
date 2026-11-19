use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn inifra(config: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(args)
        .env("INIFRA_CONFIG_DIR", config.path())
        .env("NO_COLOR", "1")
        .output()
        .expect("run Inifra")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn write_pack(root: &Path, category: &str, id: &str, manifest: &str, files: &[(&str, &str)]) {
    let directory = root.join("templates").join(category).join(id);
    fs::create_dir_all(&directory).expect("create template directory");
    fs::write(directory.join("template.toml"), manifest).expect("write template manifest");
    for (name, contents) in files {
        let path = directory.join(name);
        fs::create_dir_all(path.parent().expect("template file parent"))
            .expect("create source parent");
        fs::write(path, contents).expect("write template source");
    }
}

#[test]
fn preset_save_list_inspect_rename_and_remove_workflows() {
    let config = TempDir::new().expect("create config");
    let saved = inifra(
        &config,
        &[
            "preset",
            "save",
            "project-docs",
            "--description",
            "Project documentation files",
            "--template",
            "agents",
            "--template",
            "readme",
            "--set",
            "project_name=example",
        ],
    );
    assert!(saved.status.success(), "{}", stderr(&saved));
    assert!(
        config
            .path()
            .join("presets/project-docs/preset.toml")
            .is_file()
    );

    let listed = inifra(&config, &["preset", "list"]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    assert!(stdout(&listed).contains("project-docs"));

    let inspected = inifra(&config, &["preset", "inspect", "project-docs"]);
    assert!(inspected.status.success(), "{}", stderr(&inspected));
    assert!(stdout(&inspected).contains("project_name = example"));

    let renamed = inifra(&config, &["preset", "rename", "project-docs", "docs"]);
    assert!(renamed.status.success(), "{}", stderr(&renamed));
    assert!(config.path().join("presets/docs/preset.toml").is_file());

    let removed = inifra(&config, &["preset", "remove", "docs"]);
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(!config.path().join("presets/docs").exists());
}

#[test]
fn template_rename_updates_pack_and_preset_references_and_remove_is_guarded() {
    let config = TempDir::new().expect("create config");
    write_pack(
        config.path(),
        "personal",
        "notes",
        "schema_version = 2\ndescription = 'Personal notes'\n",
        &[("NOTES.md.j2", "# {{ project_name }}\n")],
    );
    write_pack(
        config.path(),
        "personal",
        "overview",
        "schema_version = 2\ndescription = 'Overview'\n\n[references.notes]\ntemplate = 'notes'\nsource = 'NOTES.md.j2'\n",
        &[("README.md.j2", "See {{ references.notes }}\n")],
    );
    let saved = inifra(
        &config,
        &[
            "preset",
            "save",
            "my-notes",
            "--description",
            "Notes selection",
            "--template",
            "notes",
        ],
    );
    assert!(saved.status.success(), "{}", stderr(&saved));

    let renamed = inifra(&config, &["rename", "notes", "project-notes"]);
    assert!(renamed.status.success(), "{}", stderr(&renamed));
    assert!(stdout(&renamed).contains("updated 1 preset"));
    assert!(
        config
            .path()
            .join("templates/personal/project-notes/template.toml")
            .is_file()
    );
    let consumer_manifest = fs::read_to_string(
        config
            .path()
            .join("templates/personal/overview/template.toml"),
    )
    .unwrap();
    assert!(consumer_manifest.contains("project-notes"));
    let preset_manifest =
        fs::read_to_string(config.path().join("presets/my-notes/preset.toml")).unwrap();
    assert!(preset_manifest.contains("project-notes"));

    let blocked = inifra(&config, &["remove", "project-notes"]);
    assert!(!blocked.status.success());
    assert!(stderr(&blocked).contains("saved presets"));

    let removed_preset = inifra(&config, &["preset", "remove", "my-notes"]);
    assert!(
        removed_preset.status.success(),
        "{}",
        stderr(&removed_preset)
    );
    let blocked_by_pack = inifra(&config, &["remove", "project-notes"]);
    assert!(!blocked_by_pack.status.success());
    assert!(stderr(&blocked_by_pack).contains("referenced by templates"));

    let removed_consumer = inifra(&config, &["remove", "overview"]);
    assert!(
        removed_consumer.status.success(),
        "{}",
        stderr(&removed_consumer)
    );
    let removed_target = inifra(&config, &["remove", "project-notes"]);
    assert!(
        removed_target.status.success(),
        "{}",
        stderr(&removed_target)
    );
}

#[test]
fn preset_apply_requires_the_same_interactive_review_as_init() {
    let config = TempDir::new().expect("create config");
    let saved = inifra(
        &config,
        &[
            "preset",
            "save",
            "editor-settings",
            "--description",
            "Editor defaults",
            "--template",
            "editorconfig",
        ],
    );
    assert!(saved.status.success(), "{}", stderr(&saved));
    let target = config.path().join("target");

    let applied = inifra(
        &config,
        &[
            "preset",
            "apply",
            "editor-settings",
            "--target",
            target.to_str().unwrap(),
        ],
    );

    assert!(!applied.status.success());
    assert!(stderr(&applied).contains("requires an interactive terminal"));
    assert!(!target.exists());
}

#[cfg(unix)]
#[test]
fn editing_a_bundled_template_copies_it_before_opening_the_editor() {
    use std::os::unix::fs::PermissionsExt;

    let config = TempDir::new().expect("create config");
    let editor = config.path().join("editor");
    fs::write(
        &editor,
        "#!/bin/sh\nprintf '\\n# Edited from Inifra\\n' >> \"$1\"\n",
    )
    .expect("write fake editor");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755))
        .expect("make editor executable");

    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["edit", "readme"])
        .env("INIFRA_CONFIG_DIR", config.path())
        .env("VISUAL", &editor)
        .env("EDITOR", &editor)
        .env("NO_COLOR", "1")
        .output()
        .expect("run Inifra");

    assert!(output.status.success(), "{}", stderr(&output));
    let copied = config
        .path()
        .join("templates/project-files/readme/template.toml");
    assert!(copied.is_file());
    assert!(
        fs::read_to_string(copied)
            .expect("read copied manifest")
            .contains("Edited from Inifra")
    );
}

#[test]
fn unknown_template_edit_does_not_create_data_without_a_terminal() {
    let config = TempDir::new().expect("create config");

    let output = inifra(&config, &["edit", "my-template"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("in a terminal to create it"));
    assert!(!config.path().join("templates").exists());
}

#[test]
fn malformed_preset_can_be_removed_from_the_cli() {
    let config = TempDir::new().expect("create config");
    let directory = config.path().join("presets/broken");
    fs::create_dir_all(&directory).expect("create preset directory");
    fs::write(directory.join("preset.toml"), "not = [valid TOML").expect("write bad preset");

    let removed = inifra(&config, &["preset", "remove", "broken"]);

    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(!directory.exists());
}

#[test]
fn template_and_preset_management_remain_scoped_to_the_default_catalog() {
    let config = TempDir::new().expect("create config");
    let catalog = config.path().join("catalog");
    write_pack(
        config.path(),
        "custom",
        "notes",
        "schema_version = 2\ndescription = 'Notes'\n",
        &[("NOTES.md", "Notes\n")],
    );
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--catalog", catalog.to_str().unwrap(), "preset", "list"])
        .env("INIFRA_CONFIG_DIR", config.path())
        .env("NO_COLOR", "1")
        .output()
        .expect("run Inifra");

    assert!(!output.status.success());
    assert!(stderr(&output).contains("cannot be used with --catalog"));
}
