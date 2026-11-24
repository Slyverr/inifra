use inifra_assistant::{
    ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Provider,
};

fn profile(provider: Provider, api: ApiProtocol) -> AssistantProfile {
    AssistantProfile {
        provider,
        api,
        model: "example-model".into(),
        base_url: matches!(
            provider,
            Provider::OpenAiCompatible | Provider::CustomEndpoint
        )
        .then(|| "https://llm.example/v1/".into()),
        api_key_env: (!matches!(provider, Provider::Ollama | Provider::CustomEndpoint))
            .then(|| "EXAMPLE_API_KEY".into()),
        credential_source: CredentialSource::Environment,
    }
}

#[test]
fn profile_config_round_trips_and_keeps_credentials_as_references() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("assistant.toml");
    let mut config = AssistantConfig::default();
    config
        .insert_profile(
            "work",
            profile(
                Provider::OpenAiCompatible,
                ApiProtocol::OpenAiChatCompletions,
            ),
        )
        .unwrap();
    config.save(&path).unwrap();

    let serialized = std::fs::read_to_string(&path).unwrap();
    assert!(serialized.contains("api_key_env = \"EXAMPLE_API_KEY\""));
    assert!(!serialized.contains("secret"));
    let loaded = AssistantConfig::load(&path).unwrap().unwrap();
    assert_eq!(loaded.default_profile_id(), Some("work"));
    assert_eq!(loaded.profile("work"), config.profile("work"));
}

#[test]
fn profile_selection_and_removal_keep_a_valid_default() {
    let mut config = AssistantConfig::default();
    config
        .insert_profile(
            "first",
            profile(Provider::OpenAi, ApiProtocol::OpenAiResponses),
        )
        .unwrap();
    config
        .insert_profile("second", profile(Provider::Ollama, ApiProtocol::OllamaChat))
        .unwrap();
    config.set_default_profile("second").unwrap();
    config.remove_profile("second").unwrap();

    assert_eq!(config.default_profile_id(), Some("first"));
    assert!(config.validate().is_ok());
}

#[test]
fn rejects_unsupported_protocols_and_invalid_profile_values() {
    let mut invalid = profile(Provider::Anthropic, ApiProtocol::OpenAiChatCompletions);
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .to_string()
            .contains("not supported")
    );

    invalid = profile(
        Provider::OpenAiCompatible,
        ApiProtocol::OpenAiChatCompletions,
    );
    invalid.api_key_env = Some("api-key".into());
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .to_string()
            .contains("environment variable")
    );

    invalid = profile(Provider::OpenAi, ApiProtocol::OpenAiResponses);
    invalid.base_url = Some("https://user:secret@example.com/v1".into());
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .to_string()
            .contains("credentials")
    );

    invalid.base_url = Some("https://@example.com/v1".into());
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .to_string()
            .contains("credentials")
    );
}

#[test]
fn custom_api_profiles_accept_every_protocol_with_optional_credentials() {
    for api in [
        ApiProtocol::OpenAiResponses,
        ApiProtocol::OpenAiChatCompletions,
        ApiProtocol::AnthropicMessages,
        ApiProtocol::GeminiGenerateContent,
        ApiProtocol::OllamaChat,
    ] {
        let mut profile = profile(Provider::CustomEndpoint, api);
        profile.api_key_env = None;
        assert!(profile.validate().is_ok(), "{api:?}");
    }

    let mut profile = profile(Provider::CustomEndpoint, ApiProtocol::OpenAiResponses);
    profile.base_url = None;
    assert!(
        profile
            .validate()
            .unwrap_err()
            .to_string()
            .contains("base_url")
    );
}

#[test]
fn config_rejects_unknown_fields_and_unsupported_schema_versions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("assistant.toml");
    std::fs::write(&path, "schema_version = 2\n").unwrap();
    assert!(
        AssistantConfig::load(&path)
            .unwrap_err()
            .to_string()
            .contains("schema version")
    );

    std::fs::write(
        &path,
        "schema_version = 1\napi_key = 'should never be saved here'\n",
    )
    .unwrap();
    assert!(
        AssistantConfig::load(&path)
            .unwrap_err()
            .to_string()
            .contains("unknown field")
    );
}

#[test]
fn rejects_profile_ids_that_are_not_stable_lowercase_names() {
    let mut config = AssistantConfig::default();
    assert!(
        config
            .insert_profile(
                "Team Profile",
                profile(Provider::Ollama, ApiProtocol::OllamaChat)
            )
            .is_err()
    );
}
