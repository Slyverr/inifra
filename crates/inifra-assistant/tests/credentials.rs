use inifra_assistant::{
    ApiProtocol, AssistantProfile, CredentialSource, Provider, read_credential, resolve_credential,
    write_credential,
};

fn profile(source: CredentialSource) -> AssistantProfile {
    AssistantProfile {
        provider: Provider::OpenAi,
        api: ApiProtocol::OpenAiResponses,
        model: "example-model".into(),
        base_url: None,
        api_key_env: Some("OPENAI_API_KEY".into()),
        credential_source: source,
    }
}

#[test]
fn credentials_are_literal_and_environment_values_take_precedence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials.env");
    write_credential(&path, "OPENAI_API_KEY", "literal $value with spaces").unwrap();
    assert_eq!(
        read_credential(&path, "OPENAI_API_KEY").unwrap().as_deref(),
        Some("literal $value with spaces")
    );

    let from_environment =
        resolve_credential(&profile(CredentialSource::CredentialsFile), &path, |_| {
            Some("environment-key".into())
        })
        .unwrap();
    assert_eq!(from_environment.as_deref(), Some("environment-key"));

    let from_file =
        resolve_credential(&profile(CredentialSource::CredentialsFile), &path, |_| None).unwrap();
    assert_eq!(from_file.as_deref(), Some("literal $value with spaces"));
}

#[test]
fn environment_only_profiles_do_not_read_credentials_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials.env");
    assert!(
        resolve_credential(&profile(CredentialSource::Environment), &path, |_| None)
            .unwrap_err()
            .to_string()
            .contains("OPENAI_API_KEY")
    );
    assert!(!path.exists());
}

#[test]
fn credential_file_update_preserves_other_names_and_uses_private_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials.env");
    write_credential(&path, "FIRST_KEY", "first").unwrap();
    write_credential(&path, "SECOND_KEY", "second").unwrap();
    assert_eq!(
        read_credential(&path, "FIRST_KEY").unwrap().as_deref(),
        Some("first")
    );
    assert_eq!(
        read_credential(&path, "SECOND_KEY").unwrap().as_deref(),
        Some("second")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[test]
fn credentials_reject_symlinks_and_files_readable_by_others() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.env");
    std::fs::write(&target, "OPENAI_API_KEY=secret\n").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_credential(&target, "OPENAI_API_KEY").is_err());

    let link = directory.path().join("credentials.env");
    symlink(&target, &link).unwrap();
    assert!(read_credential(&link, "OPENAI_API_KEY").is_err());
}

#[test]
fn credential_values_cannot_add_lines_or_be_empty() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials.env");
    assert!(write_credential(&path, "OPENAI_API_KEY", "").is_err());
    assert!(write_credential(&path, "OPENAI_API_KEY", "key\nOTHER=value").is_err());
}
