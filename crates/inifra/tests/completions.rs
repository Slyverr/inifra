use std::{
    fs,
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
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .args(["completions", "fish", "--install"])
        .env("INIFRA_CONFIG_DIR", config.path().join("inifra"))
        .env("XDG_CONFIG_HOME", &xdg_config)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("NO_COLOR", "1")
        .output()
        .expect("run Inifra");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.is_file());
}
