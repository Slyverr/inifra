use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

fn run_cli(config: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(arguments)
        .env("INIFRA_CONFIG_DIR", config)
        .output()
        .expect("run Inifra")
}

fn git(repository: &Path, arguments: &[&str]) -> Output {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "Git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn make_template_repository(repository: &Path, body: &str) -> String {
    let pack = repository.join("templates/notes");
    fs::create_dir_all(&pack).expect("create source template pack");
    fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"notes\"\ndescription = \"Project notes\"\n\n[[files]]\nsource = \"NOTES.md\"\noutput = \"NOTES.md\"\n",
    )
    .expect("write source manifest");
    fs::write(pack.join("NOTES.md"), body).expect("write source template");
    git(repository, &["init", "--quiet"]);
    git(repository, &["add", "--all"]);
    commit(repository, "add test template");
    String::from_utf8(git(repository, &["rev-parse", "HEAD"]).stdout)
        .expect("commit ID is UTF-8")
        .trim()
        .to_owned()
}

fn commit(repository: &Path, message: &str) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args([
            "-c",
            "user.name=Inifra tests",
            "-c",
            "user.email=inifra-tests@example.invalid",
            "commit",
            "--quiet",
            "-m",
        ])
        .arg(message)
        .output()
        .expect("commit source template");
    assert!(
        output.status.success(),
        "Git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn project_source_is_pinned_namespaced_and_usable_offline() {
    let temporary = tempdir().expect("create temporary directory");
    let repository = temporary.path().join("template-source");
    fs::create_dir(&repository).expect("create source repository");
    make_template_repository(&repository, "Shared notes\n");
    let archive = repository.join("templates/archive");
    fs::create_dir_all(&archive).expect("create archive template");
    fs::write(
        archive.join("template.toml"),
        "schema_version = 1\nid = \"archive\"\ndescription = \"Archived notes\"\n\n[[files]]\nsource = \"ARCHIVE.md\"\noutput = \"ARCHIVE.md\"\n",
    )
    .expect("write archive manifest");
    fs::write(archive.join("ARCHIVE.md"), "Archived\n").expect("write archive template");
    git(&repository, &["add", "--all"]);
    commit(&repository, "add archive template");
    let revision = String::from_utf8(git(&repository, &["rev-parse", "HEAD"]).stdout)
        .expect("initial commit ID is UTF-8")
        .trim()
        .to_owned();
    let project = temporary.path().join("project");
    let config = temporary.path().join("config");
    fs::create_dir(&project).expect("create project");

    let repository = repository.to_string_lossy().into_owned();
    let project_path = project.to_string_lossy().into_owned();
    let added = run_cli(
        &config,
        &[
            "source",
            "add",
            "team",
            &repository,
            "--revision",
            "HEAD",
            "--path",
            &project_path,
        ],
    );
    assert!(
        added.status.success(),
        "source add failed: {}",
        String::from_utf8_lossy(&added.stderr)
    );
    let registry = fs::read_to_string(project.join(".inifra/sources.toml"))
        .expect("read project source registry");
    assert!(registry.contains(&revision));

    fs::write(
        Path::new(&repository).join("templates/notes/NOTES.md"),
        "Updated shared notes\n",
    )
    .expect("update source template");
    let guide = Path::new(&repository).join("templates/guide");
    fs::create_dir_all(&guide).expect("create guide template");
    fs::write(
        guide.join("template.toml"),
        "schema_version = 1\nid = \"guide\"\ndescription = \"Project guide\"\n\n[[files]]\nsource = \"GUIDE.md\"\noutput = \"GUIDE.md\"\n",
    )
    .expect("write guide manifest");
    fs::write(guide.join("GUIDE.md"), "Guide\n").expect("write guide template");
    fs::remove_dir_all(Path::new(&repository).join("templates/archive"))
        .expect("remove archive template");
    git(Path::new(&repository), &["add", "--all"]);
    commit(Path::new(&repository), "update test template");
    let updated_revision =
        String::from_utf8(git(Path::new(&repository), &["rev-parse", "HEAD"]).stdout)
            .expect("updated commit ID is UTF-8")
            .trim()
            .to_owned();
    let update = run_cli(
        &config,
        &[
            "source",
            "update",
            "team",
            "--revision",
            "HEAD",
            "--path",
            &project_path,
        ],
    );
    assert!(
        update.status.success(),
        "source update failed: {}",
        String::from_utf8_lossy(&update.stderr)
    );
    let registry = fs::read_to_string(project.join(".inifra/sources.toml"))
        .expect("read updated project source registry");
    assert!(registry.contains(&updated_revision));
    assert!(!registry.contains(&revision));
    let update_output = String::from_utf8_lossy(&update.stdout);
    assert!(
        update_output.contains("Updated templates: notes"),
        "{update_output}"
    );
    assert!(
        update_output.contains("Added templates: guide"),
        "{update_output}"
    );
    assert!(
        update_output.contains("Removed templates: archive"),
        "{update_output}"
    );

    let unchanged = run_cli(
        &config,
        &[
            "source",
            "update",
            "team",
            "--revision",
            "HEAD",
            "--path",
            &project_path,
        ],
    );
    assert!(unchanged.status.success());
    assert!(String::from_utf8_lossy(&unchanged.stdout).contains("No template changes."));

    let saved = run_cli(
        &config,
        &[
            "preset",
            "save",
            "shared-notes",
            "--template",
            "team/notes",
            "--path",
            &project_path,
        ],
    );
    assert!(
        saved.status.success(),
        "preset save failed: {}",
        String::from_utf8_lossy(&saved.stderr)
    );

    let preset_project = temporary.path().join("preset-project");
    fs::create_dir(&preset_project).expect("create preset project");
    fs::create_dir(preset_project.join(".inifra")).expect("create preset project config");
    fs::copy(
        project.join(".inifra/sources.toml"),
        preset_project.join(".inifra/sources.toml"),
    )
    .expect("copy pinned source declarations");
    let preset_project_argument = preset_project.to_string_lossy().into_owned();
    let applied = run_cli(
        &config,
        &[
            "init",
            "--preset",
            "shared-notes",
            "--path",
            &preset_project_argument,
        ],
    );
    assert!(
        applied.status.success(),
        "preset apply failed: {}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(
        fs::read_to_string(preset_project.join("NOTES.md")).expect("read preset output"),
        "Updated shared notes\n"
    );

    fs::remove_dir_all(&repository).expect("remove remote to verify offline use");
    let listing = run_cli(&config, &["list", "--path", &project_path]);
    assert!(
        listing.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&listing.stderr)
    );
    assert!(String::from_utf8_lossy(&listing.stdout).contains("team/notes"));

    let validation = run_cli(
        &config,
        &["validate", "team/notes", "--path", &project_path],
    );
    assert!(
        validation.status.success(),
        "validation failed: {}",
        String::from_utf8_lossy(&validation.stderr)
    );
    assert!(String::from_utf8_lossy(&validation.stdout).contains("valid team/notes"));

    let generated = run_cli(
        &config,
        &["generate", "team/notes", "--path", &project_path],
    );
    assert!(
        generated.status.success(),
        "generate failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.join("NOTES.md")).expect("read generated notes"),
        "Updated shared notes\n"
    );
}

#[test]
fn source_sync_fetches_project_pins_into_a_fresh_user_cache() {
    let temporary = tempdir().expect("create temporary directory");
    let repository = temporary.path().join("template-source");
    fs::create_dir(&repository).expect("create source repository");
    let revision = make_template_repository(&repository, "Team notes\n");
    let first_project = temporary.path().join("first-project");
    let first_config = temporary.path().join("first-config");
    fs::create_dir(&first_project).expect("create first project");
    let repository_argument = repository.to_string_lossy().into_owned();
    let first_project_argument = first_project.to_string_lossy().into_owned();
    let added = run_cli(
        &first_config,
        &[
            "source",
            "add",
            "team",
            &repository_argument,
            "--revision",
            "HEAD",
            "--path",
            &first_project_argument,
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let second_project = temporary.path().join("second-project");
    let second_config = temporary.path().join("second-config");
    fs::create_dir(&second_project).expect("create second project");
    fs::create_dir(second_project.join(".inifra")).expect("create project Inifra config");
    fs::copy(
        first_project.join(".inifra/sources.toml"),
        second_project.join(".inifra/sources.toml"),
    )
    .expect("copy project source declaration");
    let second_project_argument = second_project.to_string_lossy().into_owned();
    let synced = run_cli(
        &second_config,
        &["source", "sync", "--path", &second_project_argument],
    );
    assert!(
        synced.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(String::from_utf8_lossy(&synced.stdout).contains("1 project template source"));
    let registry = fs::read_to_string(second_project.join(".inifra/sources.toml"))
        .expect("read copied project source registry");
    assert!(registry.contains(&revision));

    let listing = run_cli(
        &second_config,
        &["list", "--path", &second_project_argument],
    );
    assert!(listing.status.success());
    assert!(String::from_utf8_lossy(&listing.stdout).contains("team/notes"));
    let listed_sources = run_cli(
        &second_config,
        &["source", "list", "--path", &second_project_argument],
    );
    assert!(listed_sources.status.success());
    assert!(String::from_utf8_lossy(&listed_sources.stdout).contains("[synced]"));

    let removed = run_cli(
        &second_config,
        &[
            "source",
            "remove",
            "team",
            "--path",
            &second_project_argument,
        ],
    );
    assert!(removed.status.success());
    let listing = run_cli(
        &second_config,
        &["list", "--path", &second_project_argument],
    );
    assert!(listing.status.success());
    assert!(!String::from_utf8_lossy(&listing.stdout).contains("team/notes"));
}

#[cfg(unix)]
#[test]
fn source_add_rejects_symlink_entries_without_writing_a_registry() {
    use std::os::unix::fs::symlink;

    let temporary = tempdir().expect("create temporary directory");
    let repository = temporary.path().join("template-source");
    fs::create_dir(&repository).expect("create source repository");
    let pack = repository.join("templates/notes");
    fs::create_dir_all(&pack).expect("create source template pack");
    fs::write(
        pack.join("template.toml"),
        "schema_version = 1\nid = \"notes\"\ndescription = \"Project notes\"\n\n[[files]]\nsource = \"NOTES.md\"\noutput = \"NOTES.md\"\n",
    )
    .expect("write source manifest");
    fs::write(pack.join("NOTES.md"), "Notes\n").expect("write source template");
    let outside = temporary.path().join("outside");
    fs::write(&outside, "outside\n").expect("write outside file");
    symlink(&outside, pack.join("linked-file")).expect("add source symlink");
    git(&repository, &["init", "--quiet"]);
    git(&repository, &["add", "--all"]);
    let commit = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args([
            "-c",
            "user.name=Inifra tests",
            "-c",
            "user.email=inifra-tests@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "add unsafe template",
        ])
        .output()
        .expect("commit unsafe source");
    assert!(commit.status.success());

    let project = temporary.path().join("project");
    fs::create_dir(&project).expect("create project");
    let config = temporary.path().join("config");
    let repository_argument = repository.to_string_lossy().into_owned();
    let project_argument = project.to_string_lossy().into_owned();
    let output = run_cli(
        &config,
        &[
            "source",
            "add",
            "team",
            &repository_argument,
            "--revision",
            "HEAD",
            "--path",
            &project_argument,
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("symlink or non-file"));
    assert!(!project.join(".inifra/sources.toml").exists());
}
