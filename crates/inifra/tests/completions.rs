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

fn inifra_with_completion_dirs(
    config: &TempDir,
    xdg_config: &Path,
    xdg_data: &Path,
    args: &[&str],
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(args)
        .env("INIFRA_CONFIG_DIR", config.path().join("inifra"))
        .env("XDG_CONFIG_HOME", xdg_config)
        .env("XDG_DATA_HOME", xdg_data)
        .env("NO_COLOR", "1")
        .output()
        .expect("run Inifra")
}

#[test]
fn completion_script_generation_does_not_load_a_user_catalog() {
    let config = TempDir::new().expect("create config");
    let invalid = config.path().join("templates/personal/broken");
    fs::create_dir_all(&invalid).expect("create invalid pack");
    fs::write(
        invalid.join("template.toml"),
        "schema_version = 99\ndescription = 'Unsupported'\n",
    )
    .expect("write invalid manifest");

    let output = inifra(&config, &["completions", "fish"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let script = String::from_utf8(output.stdout).expect("completion script is UTF-8");
    assert!(script.contains("# Inifra generated completion file"));
    assert!(script.contains("INIFRA_COMPLETE=fish"));
}

#[test]
fn source_name_completion_uses_project_pins_without_network_access() {
    let project = TempDir::new().expect("create project");
    let config = TempDir::new().expect("create config");
    fs::create_dir_all(project.path().join(".inifra")).expect("create project config");
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
        .expect("add source pin");
    registry
        .save(&project.path().join(".inifra/sources.toml"))
        .expect("save source pins");

    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["--", "inifra", "source", "remove", "te"])
        .env("INIFRA_COMPLETE", "fish")
        .env("INIFRA_CONFIG_DIR", config.path())
        .current_dir(project.path())
        .output()
        .expect("run dynamic completion");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("team"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn installation_writes_to_the_xdg_completion_directory() {
    let config = TempDir::new().expect("create config");
    let xdg_config = config.path().join("config");
    let xdg_data = config.path().join("data");
    let target = xdg_config.join("fish/completions/inifra.fish");
    let output = inifra_with_completion_dirs(
        &config,
        &xdg_config,
        &xdg_data,
        &["completions", "fish", "--install"],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.is_file());
    assert!(!xdg_config.join("fish/config.fish").exists());
}

#[test]
fn installation_requires_overwrite_for_an_unmanaged_file() {
    let config = TempDir::new().expect("create config");
    let xdg_config = config.path().join("config");
    let xdg_data = config.path().join("data");
    let target = xdg_config.join("fish/completions/inifra.fish");
    fs::create_dir_all(target.parent().unwrap()).expect("create completion directory");
    fs::write(&target, "custom completion\n").expect("write user completion");

    let output = inifra_with_completion_dirs(
        &config,
        &xdg_config,
        &xdg_data,
        &["completions", "fish", "--install"],
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--overwrite"));
    assert_eq!(fs::read_to_string(target).unwrap(), "custom completion\n");
}

#[test]
fn installation_overwrites_an_unmanaged_file_only_when_requested() {
    let config = TempDir::new().expect("create config");
    let xdg_config = config.path().join("config");
    let xdg_data = config.path().join("data");
    let target = xdg_config.join("fish/completions/inifra.fish");
    fs::create_dir_all(target.parent().unwrap()).expect("create completion directory");
    fs::write(&target, "custom completion\n").expect("write user completion");

    let output = inifra_with_completion_dirs(
        &config,
        &xdg_config,
        &xdg_data,
        &["completions", "fish", "--install", "--overwrite"],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = fs::read_to_string(target).unwrap();
    assert!(installed.starts_with("# Inifra generated completion file\n"));
    assert!(installed.contains("INIFRA_COMPLETE=fish"));
}

#[test]
fn uninstall_removes_a_marked_completion_file() {
    let config = TempDir::new().expect("create config");
    let xdg_config = config.path().join("config");
    let xdg_data = config.path().join("data");
    let target = xdg_config.join("fish/completions/inifra.fish");

    let installed = inifra_with_completion_dirs(
        &config,
        &xdg_config,
        &xdg_data,
        &["completions", "fish", "--install"],
    );
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    assert!(target.is_file());

    let removed = inifra_with_completion_dirs(
        &config,
        &xdg_config,
        &xdg_data,
        &["completions", "fish", "--uninstall"],
    );

    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!target.exists());
}

#[test]
fn uninstall_requires_force_for_an_unmanaged_file() {
    let config = TempDir::new().expect("create config");
    let xdg_config = config.path().join("config");
    let xdg_data = config.path().join("data");
    let target = xdg_config.join("fish/completions/inifra.fish");
    fs::create_dir_all(target.parent().unwrap()).expect("create completion directory");
    fs::write(&target, "custom completion\n").expect("write user completion");

    let output = inifra_with_completion_dirs(
        &config,
        &xdg_config,
        &xdg_data,
        &["completions", "fish", "--uninstall"],
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--force"));
    assert_eq!(fs::read_to_string(target).unwrap(), "custom completion\n");
}

#[test]
fn uninstall_removes_an_unmanaged_file_only_when_forced() {
    let config = TempDir::new().expect("create config");
    let xdg_config = config.path().join("config");
    let xdg_data = config.path().join("data");
    let target = xdg_config.join("fish/completions/inifra.fish");
    fs::create_dir_all(target.parent().unwrap()).expect("create completion directory");
    fs::write(&target, "custom completion\n").expect("write user completion");

    let output = inifra_with_completion_dirs(
        &config,
        &xdg_config,
        &xdg_data,
        &["completions", "fish", "--uninstall", "--force"],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!target.exists());
}

#[test]
fn overwrite_and_force_are_limited_to_their_operation() {
    let config = TempDir::new().expect("create config");
    let xdg_config = config.path().join("config");
    let xdg_data = config.path().join("data");

    for args in [
        &["completions", "fish", "--overwrite"][..],
        &["completions", "fish", "--force"][..],
        &["completions", "fish", "--install", "--force"][..],
        &["completions", "fish", "--uninstall", "--overwrite"][..],
    ] {
        let output = inifra_with_completion_dirs(&config, &xdg_config, &xdg_data, args);
        assert!(
            !output.status.success(),
            "accepted invalid flags {args:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
