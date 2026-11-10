use std::process::{Command, Output};

use tempfile::tempdir;

fn run_cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(arguments)
        .output()
        .expect("run Inifra")
}

fn run_cli_in(arguments: &[&str], directory: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("run Inifra")
}

#[test]
fn help_lists_available_commands() {
    let output = run_cli(&["--help"]);

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("list"));
    assert!(help.contains("generate"));
}

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
    assert!(listing.contains("editorconfig"));
    assert!(listing.contains(".editorconfig"));
    assert!(listing.contains("gitignore-rust"));
    assert!(listing.contains(".gitignore"));
}

#[test]
fn list_is_independent_of_the_current_directory() {
    let destination = tempdir().expect("create temporary directory");
    let output = run_cli_in(&["list"], destination.path());

    assert!(output.status.success());
    assert!(!destination.path().join("CHANGELOG.md").exists());
}

#[test]
fn generate_writes_template_to_the_requested_directory() {
    let destination = tempdir().expect("create temporary directory");
    let output = run_cli(&[
        "generate",
        "gitignore-rust",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Wrote 1 file"));
    assert_eq!(
        std::fs::read_to_string(destination.path().join(".gitignore")).expect("read output"),
        "/target/\n"
    );
}

#[test]
fn generate_dry_run_does_not_create_the_destination() {
    let parent = tempdir().expect("create temporary directory");
    let destination = parent.path().join("new-project");
    let output = run_cli(&[
        "generate",
        "changelog",
        "--path",
        destination.to_str().expect("UTF-8 path"),
        "--dry-run",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("create"));
    assert!(!destination.exists());
}

#[test]
fn generate_creates_a_missing_destination() {
    let parent = tempdir().expect("create temporary directory");
    let destination = parent.path().join("new-project");
    let output = run_cli(&[
        "generate",
        "editorconfig",
        "--path",
        destination.to_str().expect("UTF-8 path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(destination.join(".editorconfig").is_file());
}

#[test]
fn generate_skips_existing_files_by_default() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".gitignore");
    std::fs::write(&output_path, "keep this\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("skip (already exists)"));
    assert_eq!(
        std::fs::read_to_string(output_path).expect("read existing file"),
        "keep this\n"
    );
}

#[test]
fn generate_overwrites_only_when_requested() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".gitignore");
    std::fs::write(&output_path, "old content\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
        "--overwrite",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("replace"));
    assert_eq!(
        std::fs::read_to_string(output_path).expect("read output"),
        "/target/\n"
    );
}

#[test]
fn generate_dry_run_previews_replacement_without_changing_the_file() {
    let destination = tempdir().expect("create temporary directory");
    let output_path = destination.path().join(".gitignore");
    std::fs::write(&output_path, "old content\n").expect("write existing file");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
        "--overwrite",
        "--dry-run",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("replace"));
    assert_eq!(
        std::fs::read_to_string(output_path).expect("read existing file"),
        "old content\n"
    );
}

#[test]
fn generate_reports_unknown_template_ids_without_creating_the_destination() {
    let parent = tempdir().expect("create temporary directory");
    let destination = parent.path().join("new-project");
    let output = run_cli(&[
        "generate",
        "not-a-template",
        "--path",
        destination.to_str().expect("UTF-8 path"),
    ]);

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("unknown template"));
    assert!(error.contains("gitignore-rust"));
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn generate_refuses_to_replace_a_symlink() {
    use std::os::unix::fs::symlink;

    let destination = tempdir().expect("create destination");
    let outside = tempdir().expect("create outside directory");
    let outside_file = outside.path().join("outside.txt");
    std::fs::write(&outside_file, "keep this\n").expect("write outside file");
    symlink(&outside_file, destination.path().join(".gitignore")).expect("create symlink");

    let output = run_cli(&[
        "generate",
        "gitignore-rust",
        "--path",
        destination.path().to_str().expect("UTF-8 path"),
        "--overwrite",
    ]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("symlink"));
    assert_eq!(
        std::fs::read_to_string(outside_file).expect("read outside file"),
        "keep this\n"
    );
}
