use inifra_assistant::{
    ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Provider, read_credential,
    write_credential,
};
use inifra_tui::{FormOutcome, TextField, presentation, run_form, run_secret_input, run_select};
use std::{collections::BTreeMap, error::Error, fs, path::Path};

const API_CHOICES: &[ApiProtocol] = &[
    ApiProtocol::OpenAiResponses,
    ApiProtocol::OpenAiChatCompletions,
    ApiProtocol::AnthropicMessages,
    ApiProtocol::GeminiGenerateContent,
    ApiProtocol::OllamaChat,
];

const CHAT_PROVIDERS: &[Provider] = &[
    Provider::OpenAi,
    Provider::NvidiaNim,
    Provider::OpenRouter,
    Provider::OpenAiCompatible,
];

pub(super) fn configure_profile(
    config_path: &Path,
    credentials_path: &Path,
    mut config: AssistantConfig,
    profile_id: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let id = match profile_id {
        Some(id) => id.to_owned(),
        None => match profile_id_input()? {
            Some(id) => id,
            None => return Ok(()),
        },
    };

    let existing = config.profile(&id).cloned();

    let api_initial = existing
        .as_ref()
        .and_then(|profile| api_index(profile.api))
        .unwrap_or_default();
    let api_choices = API_CHOICES
        .iter()
        .map(|api| api.display_name().to_owned())
        .collect::<Vec<_>>();
    let Some(api_index) = run_select("API", &api_choices, api_initial)? else {
        return Ok(());
    };

    let api = API_CHOICES[api_index];

    let provider = if api == ApiProtocol::OpenAiResponses {
        Provider::OpenAi
    } else if api == ApiProtocol::OpenAiChatCompletions {
        let provider_choices = CHAT_PROVIDERS
            .iter()
            .map(|provider| provider.display_name().to_owned())
            .collect::<Vec<_>>();
        let provider_initial = existing
            .as_ref()
            .and_then(|profile| provider_index(profile.provider))
            .unwrap_or_default();
        let Some(provider_index) = run_select("Provider", &provider_choices, provider_initial)?
        else {
            return Ok(());
        };
        CHAT_PROVIDERS[provider_index]
    } else {
        provider_for_api(api)
    };

    let values = match run_form("Profile", profile_fields(provider, existing.as_ref()))? {
        FormOutcome::Submitted(values) => values,
        FormOutcome::Cancelled => return Ok(()),
    };

    let model = required_value(&values, "model")?;
    let base_url = values
        .get("base_url")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let api_key_env = if provider == Provider::Ollama {
        None
    } else {
        Some(required_value(&values, "api_key_env")?)
    };

    let (credential_source, keep_saved_credential) = if api_key_env.is_none() {
        (CredentialSource::Environment, false)
    } else {
        let mut choices = vec![
            "Read from environment".to_owned(),
            "Save in credentials.env".to_owned(),
        ];
        let can_keep = existing.as_ref().is_some_and(|profile| {
            profile.credential_source == CredentialSource::CredentialsFile
                && profile.api_key_env == api_key_env
        });
        if can_keep {
            choices.push("Keep current stored key".to_owned());
        }

        let credential_initial = existing
            .as_ref()
            .map(|profile| match profile.credential_source {
                CredentialSource::Environment => 0,
                CredentialSource::CredentialsFile if can_keep => 2,
                CredentialSource::CredentialsFile => 1,
            })
            .unwrap_or_default();
        let Some(selected) = run_select("Credential", &choices, credential_initial)? else {
            return Ok(());
        };

        let keep_saved_credential = selected == 2
            && read_credential(credentials_path, api_key_env.as_deref().unwrap_or_default())?
                .is_some_and(|secret| !secret.trim().is_empty());
        match selected {
            0 => (CredentialSource::Environment, false),
            1 => (CredentialSource::CredentialsFile, false),
            _ => (CredentialSource::CredentialsFile, keep_saved_credential),
        }
    };

    let profile = AssistantProfile {
        provider,
        api,
        model,
        base_url,
        api_key_env: api_key_env.clone(),
        credential_source,
    };
    profile.validate()?;

    let secret = if credential_source == CredentialSource::CredentialsFile && !keep_saved_credential
    {
        Some(
            match run_secret_input(&format!(
                "API key for {}",
                api_key_env.as_deref().unwrap_or_default()
            ))? {
                FormOutcome::Submitted(values) => required_value(&values, "secret")?,
                FormOutcome::Cancelled => return Ok(()),
            },
        )
    } else {
        None
    };

    let original = AssistantConfig::load(config_path)?;
    if existing.is_some() {
        config.replace_profile(&id, profile)?;
    } else {
        config.insert_profile(&id, profile)?;
    }
    config.save(config_path)?;

    if let (Some(name), Some(secret)) = (api_key_env.as_deref(), secret.as_deref())
        && let Err(error) = write_credential(credentials_path, name, secret)
    {
        restore_config(config_path, original)?;
        return Err(error.into());
    }

    let saved_profile = config
        .profile(&id)
        .ok_or("saved assistant profile is missing from the configuration")?;
    println!(
        "{} {}  {}  {}",
        presentation::paint(
            "Saved assistant profile",
            presentation::Tone::Success,
            false
        ),
        presentation::paint_emphasis(&id, presentation::Tone::Accent),
        presentation::terminal_text(saved_profile.provider.display_name()),
        presentation::terminal_text(&saved_profile.model)
    );
    Ok(())
}

pub(super) fn profile_id_input() -> Result<Option<String>, Box<dyn Error>> {
    match run_form(
        "Profile",
        vec![TextField {
            name: "id".to_owned(),
            label: "Profile ID (lowercase letters, numbers, hyphens)".to_owned(),
            required: true,
            default: None,
        }],
    )? {
        FormOutcome::Submitted(values) => Ok(Some(required_value(&values, "id")?)),
        FormOutcome::Cancelled => Ok(None),
    }
}

fn restore_config(path: &Path, original: Option<AssistantConfig>) -> Result<(), Box<dyn Error>> {
    match original {
        Some(config) => config.save(path)?,
        None => match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        },
    }

    Ok(())
}

fn profile_fields(provider: Provider, existing: Option<&AssistantProfile>) -> Vec<TextField> {
    let existing = existing.filter(|profile| profile.provider == provider);
    let mut fields = vec![TextField {
        name: "model".to_owned(),
        label: "Model ID".to_owned(),
        required: true,
        default: existing.map(|profile| profile.model.clone()),
    }];
    fields.push(TextField {
        name: "base_url".to_owned(),
        label: "API base URL".to_owned(),
        required: provider == Provider::OpenAiCompatible,
        default: existing.and_then(|profile| profile.base_url.clone()),
    });
    if provider != Provider::Ollama {
        fields.push(TextField {
            name: "api_key_env".to_owned(),
            label: "API key environment variable".to_owned(),
            required: true,
            default: existing
                .and_then(|profile| profile.api_key_env.clone())
                .or_else(|| Some(default_key_name(provider).to_owned())),
        });
    }
    fields
}

fn default_key_name(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi | Provider::OpenAiCompatible => "OPENAI_API_KEY",
        Provider::NvidiaNim => "NVIDIA_API_KEY",
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::Gemini => "GEMINI_API_KEY",
        Provider::Ollama => "",
        Provider::OpenRouter => "OPENROUTER_API_KEY",
    }
}

fn api_index(api: ApiProtocol) -> Option<usize> {
    API_CHOICES.iter().position(|choice| *choice == api)
}

fn provider_index(provider: Provider) -> Option<usize> {
    CHAT_PROVIDERS.iter().position(|choice| *choice == provider)
}

fn provider_for_api(api: ApiProtocol) -> Provider {
    match api {
        ApiProtocol::OpenAiResponses => Provider::OpenAi,
        ApiProtocol::OpenAiChatCompletions => unreachable!("chat provider is selected separately"),
        ApiProtocol::AnthropicMessages => Provider::Anthropic,
        ApiProtocol::GeminiGenerateContent => Provider::Gemini,
        ApiProtocol::OllamaChat => Provider::Ollama,
    }
}

fn required_value(values: &BTreeMap<String, String>, key: &str) -> Result<String, Box<dyn Error>> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing form value {key:?}").into())
}

#[cfg(test)]
mod tests {
    use super::profile_fields;
    use inifra_assistant::Provider;

    #[test]
    fn custom_endpoints_require_a_base_url_and_default_the_key_name() {
        let fields = profile_fields(Provider::OpenAiCompatible, None);
        let base_url = fields
            .iter()
            .find(|field| field.name == "base_url")
            .unwrap();
        let api_key = fields
            .iter()
            .find(|field| field.name == "api_key_env")
            .unwrap();

        assert!(base_url.required);
        assert_eq!(api_key.default.as_deref(), Some("OPENAI_API_KEY"));
    }

    #[test]
    fn local_ollama_profiles_do_not_request_an_api_key() {
        let fields = profile_fields(Provider::Ollama, None);
        assert!(!fields.iter().any(|field| field.name == "api_key_env"));
    }
}
