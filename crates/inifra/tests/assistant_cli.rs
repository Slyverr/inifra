use inifra_assistant::{
    ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Provider,
};
use std::{fs, path::Path, process::Command};

fn inifra(config_dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.env("INIFRA_CONFIG_DIR", config_dir);
    command
}

fn profile(model: &str) -> AssistantProfile {
    AssistantProfile {
        provider: Provider::OpenAi,
        api: ApiProtocol::OpenAiResponses,
        model: model.to_owned(),
        base_url: None,
        api_key_env: Some("OPENAI_API_KEY".to_owned()),
        credential_source: CredentialSource::Environment,
    }
}

#[test]
fn profiles_lists_an_empty_assistant_configuration() {
    let config_dir = tempfile::tempdir().unwrap();
    let output = inifra(config_dir.path())
        .args(["assistant", "profiles"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("No assistant profiles configured"));
}

#[test]
fn assistant_help_lists_profile_actions() {
    let config_dir = tempfile::tempdir().unwrap();
    let output = inifra(config_dir.path())
        .args(["assistant", "--help"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for command in ["profiles", "use", "configure", "remove", "reset"] {
        assert!(help.contains(command), "{help}");
    }
}

#[test]
fn use_switches_the_default_profile() {
    let config_dir = tempfile::tempdir().unwrap();
    let path = config_dir.path().join("assistant.toml");
    let mut config = AssistantConfig::default();
    config
        .insert_profile("default", profile("model-a"))
        .unwrap();
    config.insert_profile("local", profile("model-b")).unwrap();
    config.save(&path).unwrap();

    let output = inifra(config_dir.path())
        .args(["assistant", "use", "local"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        AssistantConfig::load(&path)
            .unwrap()
            .unwrap()
            .default_profile_id(),
        Some("local")
    );
}

#[test]
fn profiles_never_print_saved_api_keys() {
    let config_dir = tempfile::tempdir().unwrap();
    let path = config_dir.path().join("assistant.toml");
    let mut profile = profile("model-a");
    profile.credential_source = CredentialSource::CredentialsFile;
    let mut config = AssistantConfig::default();
    config.insert_profile("default", profile).unwrap();
    config.save(&path).unwrap();
    fs::write(
        config_dir.path().join("credentials.env"),
        "OPENAI_API_KEY=do-not-print-this-secret\n",
    )
    .unwrap();

    let output = inifra(config_dir.path())
        .args(["assistant", "profiles"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let listing = String::from_utf8_lossy(&output.stdout);
    assert!(listing.contains("default"));
    assert!(listing.contains("OpenAI"));
    assert!(listing.contains("Responses API"));
    assert!(listing.contains("model-a"));
    assert!(!listing.contains("do-not-print-this-secret"));
}

#[test]
fn reset_requires_a_terminal_and_preserves_data_when_unattended() {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("assistant.toml");
    let credentials_path = config_dir.path().join("credentials.env");
    let mut config = AssistantConfig::default();
    config
        .insert_profile("default", profile("model-a"))
        .unwrap();
    config.save(&config_path).unwrap();
    fs::write(&credentials_path, "OPENAI_API_KEY=secret\n").unwrap();

    let output = inifra(config_dir.path())
        .args(["assistant", "reset"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires an interactive terminal"));
    assert!(config_path.is_file());
    assert!(credentials_path.is_file());
}
