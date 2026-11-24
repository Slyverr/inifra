use super::UiExit;
use inifra_assistant::{
    ApiProtocol, AssistantConfig, AssistantProfile, ConnectionTemplate, CredentialSource, Effort,
    PROVIDER_PRESETS, Provider, ProviderPreset, connection_template, is_valid_profile_id,
    read_credential, write_credential,
};
use inifra_tui::{
    Badge, FormOutcome, Heading, InteractionOutcome, Row, SelectScreen, TextField, presentation,
    presentation::Tone, run_form, run_secret_input, run_select,
};
use std::{collections::BTreeMap, error::Error, fs, path::Path};

pub(super) fn configure_profile(
    config_path: &Path,
    credentials_path: &Path,
    mut config: AssistantConfig,
    profile_id: Option<&str>,
) -> Result<UiExit, Box<dyn Error>> {
    let presets = PROVIDER_PRESETS
        .iter()
        .filter(|preset| preset.selectable)
        .copied()
        .collect::<Vec<_>>();
    let existing = profile_id.and_then(|id| config.profile(id)).cloned();
    let task = if existing.is_some() {
        "edit profile"
    } else {
        "new profile"
    };
    let heading = |id: Option<&str>| {
        let heading = Heading::new(task);
        match id {
            Some(id) => heading.context(id),
            None => heading,
        }
    };

    let mut choice = ConnectionChoice::initial(existing.as_ref(), &presets);

    let (id, template, values, api_key_env, credential_source, secret) = 'selection: loop {
        let template = match select_connection(
            &heading(profile_id),
            &mut choice,
            &presets,
            existing.as_ref(),
        )? {
            Selected::Template(template) => template,
            Selected::Back => return Ok(UiExit::Continue),
            Selected::Quit => return Ok(UiExit::Quit),
        };
        let form_heading = heading(profile_id)
            .context(template.provider.display_name())
            .context(template.api.short_name());
        let mut fields = Vec::new();
        if profile_id.is_none() {
            fields.push(profile_id_field(&config));
        }
        fields.extend(profile_fields(template, existing.as_ref()));

        let mut values = match run_form(&form_heading, fields.clone())? {
            FormOutcome::Submitted(values) => values,
            FormOutcome::Back => {
                choice.resume_at_list = true;
                continue 'selection;
            }
            FormOutcome::Quit => return Ok(UiExit::Quit),
        };
        let mut credential_initial = existing
            .as_ref()
            .map(|profile| match profile.credential_source {
                CredentialSource::Environment => 0,
                CredentialSource::CredentialsFile => 1,
            })
            .unwrap_or_default();

        let (id, api_key_env, credential_source, secret) = 'credentials: loop {
            let id = match profile_id {
                Some(id) => id.to_owned(),
                None => normalize_profile_name(&required_value(&values, "id")?),
            };
            let api_key_env = profile_api_key_env(template, &values)?;
            let Some(api_key_env) = api_key_env else {
                break (id, None, CredentialSource::Environment, None);
            };

            let mut choices = vec![
                Row::new("Read from environment").badge(environment_badge(&api_key_env)),
                Row::new("Save in credentials.env")
                    .detail("kept in the Inifra config directory, never in assistant.toml"),
            ];
            let can_keep = existing.as_ref().is_some_and(|profile| {
                profile.credential_source == CredentialSource::CredentialsFile
                    && profile.api_key_env.as_deref() == Some(&api_key_env)
            });
            if can_keep {
                choices.push(Row::new("Keep current stored key").detail("already saved"));
            }
            if !can_keep && credential_initial > 1 {
                credential_initial = 1;
            }

            let credential_heading = heading(Some(&id)).context(format!("API key {api_key_env}"));
            let screen = SelectScreen {
                heading: credential_heading.clone(),
                choices,
                initial: credential_initial,
                ..SelectScreen::default()
            };
            let selected = match run_select(&screen)? {
                InteractionOutcome::Selected(selected) => selected,
                InteractionOutcome::Back => {
                    match run_form(&form_heading, with_form_values(fields.clone(), &values))? {
                        FormOutcome::Submitted(next_values) => {
                            values = next_values;
                            continue 'credentials;
                        }
                        FormOutcome::Back => {
                            choice.resume_at_list = true;
                            continue 'selection;
                        }
                        FormOutcome::Quit => return Ok(UiExit::Quit),
                    }
                }
                InteractionOutcome::Quit => return Ok(UiExit::Quit),
            };
            credential_initial = selected;

            let credential_source = if selected == 0 {
                CredentialSource::Environment
            } else {
                CredentialSource::CredentialsFile
            };
            let keep_saved_credential = selected == 2
                && read_credential(credentials_path, &api_key_env)?
                    .is_some_and(|secret| !secret.trim().is_empty());
            let secret = if credential_source == CredentialSource::CredentialsFile
                && !keep_saved_credential
            {
                let field = TextField::new("secret", api_key_env.clone())
                    .hint("saved in credentials.env, never in assistant.toml");
                match run_secret_input(&heading(Some(&id)).context("save API key"), field)? {
                    FormOutcome::Submitted(values) => Some(required_value(&values, "secret")?),
                    FormOutcome::Back => continue 'credentials,
                    FormOutcome::Quit => return Ok(UiExit::Quit),
                }
            } else {
                None
            };

            break (id, Some(api_key_env), credential_source, secret);
        };

        break 'selection (id, template, values, api_key_env, credential_source, secret);
    };

    let model = required_value(&values, "model")?;
    let base_url = values
        .get("base_url")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let profile = AssistantProfile {
        provider: template.provider,
        api: template.api,
        model,
        base_url,
        api_key_env: api_key_env.clone(),
        credential_source,
        effort: values.get("effort").and_then(|value| Effort::parse(value)),
    };
    profile.validate()?;

    let original = AssistantConfig::load(config_path)?;
    if config.profile(&id).is_some() {
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
    println!("{}", saved_message(&id, saved_profile));
    println!("{}", default_message(&config, &id));
    Ok(UiExit::Continue)
}

/// Profile names are stored in lowercase, so `Work` and `work` are the same name.
fn normalize_profile_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn profile_id_field(config: &AssistantConfig) -> TextField {
    let taken = config.profiles().keys().cloned().collect::<Vec<_>>();
    TextField::new("id", "Profile name")
        .required()
        .hint("a short name, like work or local · letters, numbers, hyphens · saved in lowercase")
        .check(move |name| {
            let id = normalize_profile_name(name);
            if !is_valid_profile_id(&id) {
                Err("Use letters, numbers, and hyphens, starting with a letter.".to_owned())
            } else if taken.contains(&id) {
                Err(format!(
                    "A profile named {id} already exists. Choose another name, or edit it from the profile list."
                ))
            } else {
                Ok(())
            }
        })
}

/// Summarizes what a provider preset configures: protocol, endpoint host, and credential.
fn preset_detail(preset: ProviderPreset) -> String {
    let mut parts = vec![preset.default_api.short_name().to_owned()];
    if let Some(host) = preset.base_url.map(url_host) {
        parts.push(host.to_owned());
    }
    if preset.api_key_env.is_none() {
        parts.push("no key".to_owned());
    }
    parts.join(" · ")
}

/// Names the first few providers and counts the rest, keeping the row on one line.
fn known_provider_detail(presets: &[ProviderPreset]) -> String {
    const SHOWN: usize = 3;
    let names = presets
        .iter()
        .take(SHOWN)
        .map(|preset| preset.provider.display_name())
        .collect::<Vec<_>>()
        .join(", ");
    match presets.len().saturating_sub(SHOWN) {
        0 => names,
        more => format!("{names} +{more} more"),
    }
}

fn custom_endpoint_detail() -> String {
    "any compatible server · you set the URL".to_owned()
}

fn api_detail(api: ApiProtocol) -> String {
    format!("{} · you set the URL", api.display_name())
}

fn url_host(url: &str) -> &str {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_, remainder)| remainder);
    without_scheme.split('/').next().unwrap_or(without_scheme)
}

fn environment_badge(name: &str) -> Badge {
    if std::env::var_os(name).is_some_and(|value| !value.is_empty()) {
        Badge::new(format!("{name} is set"), Tone::Success)
    } else {
        Badge::new(format!("{name} is not set"), Tone::Warning)
    }
}

fn separator() -> String {
    presentation::paint(" · ", Tone::Muted, false)
}

fn saved_message(id: &str, profile: &AssistantProfile) -> String {
    format!(
        "{} {}{}{}{}{}{}{}",
        presentation::paint("Saved profile", Tone::Success, false),
        presentation::paint_emphasis(id, Tone::Accent),
        separator(),
        presentation::terminal_text(profile.provider.display_name()),
        separator(),
        presentation::terminal_text(profile.api.short_name()),
        separator(),
        presentation::terminal_text(&profile.model),
    )
}

fn default_message(config: &AssistantConfig, id: &str) -> String {
    match config.default_profile_id() {
        Some(default) if default == id => format!(
            "  {}",
            presentation::paint("now the default profile", Tone::Secondary, false)
        ),
        Some(default) => format!(
            "  {} {}{}{} {}",
            presentation::paint("default profile:", Tone::Secondary, false),
            presentation::terminal_text(default),
            separator(),
            presentation::paint("switch with", Tone::Secondary, false),
            presentation::paint(format!("inifra assistant use {id}"), Tone::Accent, false),
        ),
        None => String::new(),
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

fn profile_fields(
    template: ConnectionTemplate,
    existing: Option<&AssistantProfile>,
) -> Vec<TextField> {
    let existing = existing.filter(|profile| {
        profile.api == template.api
            && (profile.provider == template.provider
                || (template.provider == Provider::CustomEndpoint
                    && profile.provider == Provider::OpenAiCompatible
                    && template.api == ApiProtocol::OpenAiChatCompletions))
    });
    let mut fields = vec![
        TextField::new("model", "Default model")
            .required()
            .hint("switch for one session with /model in inifra ask")
            .default_value(existing.map(|profile| profile.model.clone())),
        TextField::new("effort", "Default effort")
            .hint("low, medium, or high; leave empty for the provider default")
            .default_value(
                existing
                    .and_then(|profile| profile.effort)
                    .map(|effort| effort.as_str().to_owned()),
            )
            .check(|value| {
                Effort::parse(value)
                    .map(|_| ())
                    .ok_or_else(|| "Use low, medium, or high, or leave it empty.".to_owned())
            }),
    ];
    if template.base_url_required || existing.is_some_and(|profile| profile.base_url.is_some()) {
        let field = TextField::new("base_url", "API base URL")
            .hint("the API root, such as https://host/v1")
            .default_value(existing.and_then(|profile| profile.base_url.clone()));
        fields.push(if template.base_url_required {
            field.required()
        } else {
            field
        });
    }
    if template.api_key_required
        || template.api_key_optional
        || existing.is_some_and(|profile| profile.api_key_env.is_some())
    {
        let field = TextField::new("api_key_env", "API key environment variable")
            .hint("the variable name, not the key itself")
            .default_value(
                existing
                    .and_then(|profile| profile.api_key_env.clone())
                    .or_else(|| template.api_key_env.map(str::to_owned)),
            );
        fields.push(if template.api_key_required {
            field.required()
        } else {
            field
        });
    }
    fields
}

fn with_form_values(
    mut fields: Vec<TextField>,
    values: &BTreeMap<String, String>,
) -> Vec<TextField> {
    for field in &mut fields {
        field.default = values
            .get(&field.name)
            .cloned()
            .or_else(|| field.default.clone());
    }

    fields
}

fn profile_api_key_env(
    template: ConnectionTemplate,
    values: &BTreeMap<String, String>,
) -> Result<Option<String>, Box<dyn Error>> {
    if template.api_key_required {
        Ok(Some(required_value(values, "api_key_env")?))
    } else {
        Ok(values
            .get("api_key_env")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned))
    }
}

/// Remembered answers for the connection steps, so going back keeps the previous choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConnectionChoice {
    custom: bool,
    provider_index: usize,
    api_index: usize,
    /// Skip the known/custom question and reopen the list that was last shown.
    resume_at_list: bool,
}

enum Selected {
    Template(ConnectionTemplate),
    Back,
    Quit,
}

impl ConnectionChoice {
    fn initial(existing: Option<&AssistantProfile>, presets: &[ProviderPreset]) -> Self {
        let provider_index = existing.and_then(|profile| {
            presets
                .iter()
                .position(|preset| preset.provider == profile.provider)
        });
        Self {
            custom: existing.is_some() && provider_index.is_none(),
            provider_index: provider_index.unwrap_or_default(),
            api_index: existing
                .and_then(|profile| ApiProtocol::ALL.iter().position(|api| *api == profile.api))
                .unwrap_or_default(),
            resume_at_list: false,
        }
    }
}

/// Asks whether to use a known provider or a custom endpoint, then lists only what that needs.
fn select_connection(
    heading: &Heading,
    choice: &mut ConnectionChoice,
    presets: &[ProviderPreset],
    existing: Option<&AssistantProfile>,
) -> Result<Selected, Box<dyn Error>> {
    loop {
        if !choice.resume_at_list {
            let screen = SelectScreen {
                heading: heading.clone(),
                choices: vec![
                    Row::new("Known provider").detail(known_provider_detail(presets)),
                    Row::new("Custom endpoint").detail(custom_endpoint_detail()),
                ],
                initial: usize::from(choice.custom),
                ..SelectScreen::default()
            };
            match run_select(&screen)? {
                InteractionOutcome::Selected(selected) => choice.custom = selected == 1,
                InteractionOutcome::Back => return Ok(Selected::Back),
                InteractionOutcome::Quit => return Ok(Selected::Quit),
            }
        }
        choice.resume_at_list = false;

        let (rows, initial, context): (Vec<Row>, usize, &str) = if choice.custom {
            (
                ApiProtocol::ALL
                    .iter()
                    .map(|api| Row::new(api.short_name()).detail(api_detail(*api)))
                    .collect(),
                choice.api_index,
                "custom endpoint",
            )
        } else {
            (
                presets
                    .iter()
                    .map(|preset| {
                        Row::new(preset.provider.display_name()).detail(preset_detail(*preset))
                    })
                    .collect(),
                choice.provider_index,
                "provider",
            )
        };
        let screen = SelectScreen {
            heading: heading.clone().context(context),
            choices: rows,
            initial,
            ..SelectScreen::default()
        };
        match run_select(&screen)? {
            InteractionOutcome::Selected(selected) => {
                if choice.custom {
                    choice.api_index = selected;
                } else {
                    choice.provider_index = selected;
                }
                return Ok(Selected::Template(selected_connection(
                    *choice, presets, existing,
                )?));
            }
            InteractionOutcome::Back => {}
            InteractionOutcome::Quit => return Ok(Selected::Quit),
        }
    }
}

fn selected_connection(
    choice: ConnectionChoice,
    presets: &[ProviderPreset],
    existing: Option<&AssistantProfile>,
) -> Result<ConnectionTemplate, Box<dyn Error>> {
    let template = if choice.custom {
        ApiProtocol::ALL
            .get(choice.api_index)
            .and_then(|api| connection_template(Provider::CustomEndpoint, *api))
    } else {
        let preset = presets
            .get(choice.provider_index)
            .ok_or("selected provider preset is unavailable")?;
        let api = existing
            .filter(|profile| {
                profile.provider == preset.provider && preset.apis.contains(&profile.api)
            })
            .map(|profile| profile.api)
            .unwrap_or(preset.default_api);
        preset.connection(api)
    };

    template.ok_or_else(|| "selected connection is not supported".into())
}

fn required_value(values: &BTreeMap<String, String>, key: &str) -> Result<String, Box<dyn Error>> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing form value {key:?}").into())
}

#[cfg(test)]
mod tests {
    use super::{
        ConnectionChoice, api_detail, default_message, preset_detail, profile_api_key_env,
        profile_fields, profile_id_field, saved_message, selected_connection, url_host,
    };
    use inifra_assistant::{
        ApiProtocol, AssistantConfig, AssistantProfile, ConnectionTemplate, CredentialSource,
        PROVIDER_PRESETS, Provider, connection_template,
    };
    use std::collections::BTreeMap;

    fn template(provider: Provider, api: ApiProtocol) -> ConnectionTemplate {
        connection_template(provider, api).unwrap()
    }

    #[test]
    fn custom_api_fields_require_an_endpoint_and_allow_an_optional_key() {
        let fields = profile_fields(
            template(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions),
            None,
        );
        let base_url = fields
            .iter()
            .find(|field| field.name == "base_url")
            .unwrap();
        let api_key = fields
            .iter()
            .find(|field| field.name == "api_key_env")
            .unwrap();

        assert!(base_url.required);
        assert!(!api_key.required);
    }

    #[test]
    fn local_ollama_profiles_do_not_request_an_api_key() {
        let fields = profile_fields(template(Provider::Ollama, ApiProtocol::OllamaChat), None);
        assert!(!fields.iter().any(|field| field.name == "api_key_env"));
    }

    #[test]
    fn existing_ollama_profiles_keep_an_optional_api_key_reference() {
        let values = BTreeMap::from([("api_key_env".to_owned(), "REMOTE_API_KEY".to_owned())]);
        assert_eq!(
            profile_api_key_env(template(Provider::Ollama, ApiProtocol::OllamaChat), &values)
                .unwrap()
                .as_deref(),
            Some("REMOTE_API_KEY")
        );
    }

    #[test]
    fn built_in_provider_fields_use_preset_defaults_without_asking_for_a_base_url() {
        let fields = profile_fields(
            template(Provider::NvidiaNim, ApiProtocol::OpenAiChatCompletions),
            None,
        );
        assert!(!fields.iter().any(|field| field.name == "base_url"));
        assert_eq!(
            fields
                .iter()
                .find(|field| field.name == "api_key_env")
                .unwrap()
                .default
                .as_deref(),
            Some("NVIDIA_API_KEY")
        );
    }

    #[test]
    fn choice_details_stay_short_and_name_the_first_providers() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let known = super::known_provider_detail(&presets);
        assert!(known.starts_with(presets[0].provider.display_name()));
        assert!(known.ends_with(&format!("+{} more", presets.len() - 3)));
        assert!(known.len() <= 50);
        assert!(super::custom_endpoint_detail().len() <= 50);
        for preset in &presets {
            let detail = preset_detail(*preset);
            assert!(detail.len() <= 54, "{detail}");
        }
    }

    #[test]
    fn new_profiles_start_with_a_known_provider() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let choice = ConnectionChoice::initial(None, &presets);
        assert!(!choice.custom);
        assert_eq!(choice.provider_index, 0);
        assert_eq!(choice.api_index, 0);
    }

    #[test]
    fn provider_selection_uses_the_preset_default_protocol() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let template = selected_connection(
            ConnectionChoice {
                custom: false,
                provider_index: 0,
                api_index: 2,
                resume_at_list: false,
            },
            &presets,
            None,
        )
        .unwrap();

        assert_eq!(template.provider, Provider::OpenAi);
        assert_eq!(template.api, ApiProtocol::OpenAiResponses);
    }

    #[test]
    fn protocol_selection_uses_the_shared_custom_api_template() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let template = selected_connection(
            ConnectionChoice {
                custom: true,
                provider_index: 0,
                api_index: 2,
                resume_at_list: false,
            },
            &presets,
            None,
        )
        .unwrap();

        assert_eq!(template.provider, Provider::CustomEndpoint);
        assert_eq!(template.api, ApiProtocol::AnthropicMessages);
        assert!(template.base_url_required);
    }

    #[test]
    fn existing_provider_protocol_is_preserved_when_editing_that_provider() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let existing = AssistantProfile {
            provider: Provider::OpenAi,
            api: ApiProtocol::OpenAiChatCompletions,
            model: "test-model".to_owned(),
            base_url: None,
            api_key_env: Some("OPENAI_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        };
        let template = selected_connection(
            ConnectionChoice {
                custom: false,
                provider_index: 0,
                api_index: 0,
                resume_at_list: false,
            },
            &presets,
            Some(&existing),
        )
        .unwrap();

        assert_eq!(template.provider, Provider::OpenAi);
        assert_eq!(template.api, ApiProtocol::OpenAiChatCompletions);
    }

    #[test]
    fn editing_a_legacy_custom_profile_keeps_its_values_in_the_shared_form() {
        let existing = AssistantProfile {
            provider: Provider::OpenAiCompatible,
            api: ApiProtocol::OpenAiChatCompletions,
            model: "test-model".to_owned(),
            base_url: Some("https://api.example.test/v1".to_owned()),
            api_key_env: Some("TEST_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        };
        let fields = profile_fields(
            template(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions),
            Some(&existing),
        );

        assert_eq!(
            fields
                .iter()
                .find(|field| field.name == "model")
                .unwrap()
                .default
                .as_deref(),
            Some("test-model")
        );
        assert_eq!(
            fields
                .iter()
                .find(|field| field.name == "base_url")
                .unwrap()
                .default
                .as_deref(),
            Some("https://api.example.test/v1")
        );
    }

    fn preset(provider: Provider) -> inifra_assistant::ProviderPreset {
        *PROVIDER_PRESETS
            .iter()
            .find(|preset| preset.provider == provider)
            .unwrap()
    }

    fn profile(provider: Provider, api: ApiProtocol, model: &str) -> AssistantProfile {
        AssistantProfile {
            provider,
            api,
            model: model.to_owned(),
            base_url: None,
            api_key_env: (provider != Provider::Ollama).then(|| "TEST_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        }
    }

    #[test]
    fn provider_details_name_the_protocol_host_and_credential() {
        assert_eq!(
            preset_detail(preset(Provider::OpenAi)),
            "Responses · api.openai.com"
        );
        assert_eq!(
            preset_detail(preset(Provider::Ollama)),
            "Ollama Chat · localhost:11434 · no key"
        );
        assert!(api_detail(ApiProtocol::AnthropicMessages).starts_with("Messages API · "));
        assert_eq!(url_host("https://openrouter.ai/api/v1"), "openrouter.ai");
        assert_eq!(url_host("localhost:8080"), "localhost:8080");
    }

    #[test]
    fn profile_id_field_rejects_invalid_and_taken_ids() {
        let mut config = AssistantConfig::default();
        config
            .insert_profile(
                "work",
                profile(Provider::OpenAi, ApiProtocol::OpenAiResponses, "gpt-5"),
            )
            .unwrap();
        let field = profile_id_field(&config);
        let check = field.check.as_ref().unwrap();

        assert!(field.required);
        assert!(check("1st").is_err());
        assert!(check("home lab").is_err());
        assert!(check("work").unwrap_err().contains("already exists"));
        assert!(check("Work").unwrap_err().contains("already exists"));
        assert_eq!(check("Nvidia-Test"), Ok(()));
        assert_eq!(
            super::normalize_profile_name("  Nvidia-Test "),
            "nvidia-test"
        );
        assert_eq!(check("home-lab"), Ok(()));
    }

    #[test]
    fn model_field_is_the_profile_default_and_mentions_session_switching() {
        let fields = profile_fields(
            template(Provider::OpenAi, ApiProtocol::OpenAiResponses),
            None,
        );
        assert_eq!(fields[0].label, "Default model");
        assert!(fields[0].hint.as_deref().unwrap().contains("/model"));
    }

    #[test]
    fn save_messages_name_the_connection_and_the_default() {
        let mut config = AssistantConfig::default();
        let local = profile(Provider::Ollama, ApiProtocol::OllamaChat, "qwen3:8b");
        config.insert_profile("local", local.clone()).unwrap();
        config
            .insert_profile(
                "work",
                profile(Provider::OpenAi, ApiProtocol::OpenAiResponses, "gpt-5"),
            )
            .unwrap();

        assert_eq!(
            saved_message("local", &local),
            "Saved profile local · Ollama · Ollama Chat · qwen3:8b"
        );
        assert_eq!(
            default_message(&config, "local"),
            "  now the default profile"
        );
        assert_eq!(
            default_message(&config, "work"),
            "  default profile: local · switch with inifra assistant use work"
        );
    }
}
