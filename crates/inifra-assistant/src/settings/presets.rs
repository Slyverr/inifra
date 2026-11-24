use crate::{ApiProtocol, Provider};

const OPENAI_APIS: &[ApiProtocol] = &[
    ApiProtocol::OpenAiResponses,
    ApiProtocol::OpenAiChatCompletions,
];
const CHAT_COMPLETIONS_API: &[ApiProtocol] = &[ApiProtocol::OpenAiChatCompletions];
const MESSAGES_API: &[ApiProtocol] = &[ApiProtocol::AnthropicMessages];
const GENERATE_CONTENT_API: &[ApiProtocol] = &[ApiProtocol::GeminiGenerateContent];
const OLLAMA_API: &[ApiProtocol] = &[ApiProtocol::OllamaChat];
const ALL_APIS: &[ApiProtocol] = &ApiProtocol::ALL;

/// A ready-to-use provider preset backed by one or more API protocols.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderPreset {
    /// Provider identity stored with the profile.
    pub provider: Provider,
    /// Protocols this preset configures.
    pub apis: &'static [ApiProtocol],
    /// Protocol chosen by default when this provider is selected.
    pub default_api: ApiProtocol,
    /// Default API base URL supplied by the preset.
    pub base_url: Option<&'static str>,
    /// Default environment variable name for an API key.
    pub api_key_env: Option<&'static str>,
    /// Whether profiles from this preset must name an API key variable.
    pub api_key_required: bool,
    /// Whether setup should offer an optional API-key variable field.
    pub api_key_optional: bool,
    /// Whether profiles from this preset must provide a custom base URL.
    pub base_url_required: bool,
    /// Whether Chat Completions streaming usage metadata is enabled.
    pub stream_usage: bool,
    /// Whether this preset is offered in interactive setup.
    pub selectable: bool,
}

/// The resolved provider and protocol settings used to build one assistant profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionTemplate {
    /// Selected provider preset.
    pub provider: Provider,
    /// API protocol implemented by the provider.
    pub api: ApiProtocol,
    /// Default base URL, if the provider preset supplies one.
    pub base_url: Option<&'static str>,
    /// Default API-key environment variable, if the preset uses one.
    pub api_key_env: Option<&'static str>,
    /// Whether the profile must name an API key variable.
    pub api_key_required: bool,
    /// Whether setup should offer an optional API-key variable field.
    pub api_key_optional: bool,
    /// Whether the profile must provide a custom base URL.
    pub base_url_required: bool,
    /// Whether Chat Completions streaming usage metadata is enabled.
    pub stream_usage: bool,
}

impl ProviderPreset {
    /// Resolve this provider preset for one supported protocol.
    pub fn connection(self, api: ApiProtocol) -> Option<ConnectionTemplate> {
        self.apis.contains(&api).then_some(ConnectionTemplate {
            provider: self.provider,
            api,
            base_url: self.base_url,
            api_key_env: self.api_key_env,
            api_key_required: self.api_key_required,
            api_key_optional: self.api_key_optional,
            base_url_required: self.base_url_required,
            stream_usage: self.stream_usage,
        })
    }
}

/// Provider presets used by interactive setup and profile validation.
///
/// Built-in services are defaults layered over the same protocol adapters used by custom APIs.
pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        provider: Provider::OpenAi,
        apis: OPENAI_APIS,
        default_api: ApiProtocol::OpenAiResponses,
        base_url: Some("https://api.openai.com/v1"),
        api_key_env: Some("OPENAI_API_KEY"),
        api_key_required: true,
        api_key_optional: false,
        base_url_required: false,
        stream_usage: true,
        selectable: true,
    },
    ProviderPreset {
        provider: Provider::NvidiaNim,
        apis: CHAT_COMPLETIONS_API,
        default_api: ApiProtocol::OpenAiChatCompletions,
        base_url: Some("https://integrate.api.nvidia.com/v1"),
        api_key_env: Some("NVIDIA_API_KEY"),
        api_key_required: true,
        api_key_optional: false,
        base_url_required: false,
        stream_usage: true,
        selectable: true,
    },
    ProviderPreset {
        provider: Provider::OpenRouter,
        apis: CHAT_COMPLETIONS_API,
        default_api: ApiProtocol::OpenAiChatCompletions,
        base_url: Some("https://openrouter.ai/api/v1"),
        api_key_env: Some("OPENROUTER_API_KEY"),
        api_key_required: true,
        api_key_optional: false,
        base_url_required: false,
        stream_usage: true,
        selectable: true,
    },
    ProviderPreset {
        provider: Provider::Anthropic,
        apis: MESSAGES_API,
        default_api: ApiProtocol::AnthropicMessages,
        base_url: Some("https://api.anthropic.com/v1"),
        api_key_env: Some("ANTHROPIC_API_KEY"),
        api_key_required: true,
        api_key_optional: false,
        base_url_required: false,
        stream_usage: false,
        selectable: true,
    },
    ProviderPreset {
        provider: Provider::Gemini,
        apis: GENERATE_CONTENT_API,
        default_api: ApiProtocol::GeminiGenerateContent,
        base_url: Some("https://generativelanguage.googleapis.com/v1beta"),
        api_key_env: Some("GEMINI_API_KEY"),
        api_key_required: true,
        api_key_optional: false,
        base_url_required: false,
        stream_usage: false,
        selectable: true,
    },
    ProviderPreset {
        provider: Provider::Ollama,
        apis: OLLAMA_API,
        default_api: ApiProtocol::OllamaChat,
        base_url: Some("http://localhost:11434"),
        api_key_env: None,
        api_key_required: false,
        api_key_optional: false,
        base_url_required: false,
        stream_usage: false,
        selectable: true,
    },
    ProviderPreset {
        provider: Provider::CustomEndpoint,
        apis: ALL_APIS,
        default_api: ApiProtocol::OpenAiChatCompletions,
        base_url: None,
        api_key_env: None,
        api_key_required: false,
        api_key_optional: true,
        base_url_required: true,
        stream_usage: false,
        selectable: false,
    },
    ProviderPreset {
        provider: Provider::OpenAiCompatible,
        apis: CHAT_COMPLETIONS_API,
        default_api: ApiProtocol::OpenAiChatCompletions,
        base_url: None,
        api_key_env: Some("OPENAI_API_KEY"),
        api_key_required: true,
        api_key_optional: false,
        base_url_required: true,
        stream_usage: false,
        selectable: false,
    },
];

/// Find a provider preset and resolve one supported protocol.
pub fn connection_template(provider: Provider, api: ApiProtocol) -> Option<ConnectionTemplate> {
    PROVIDER_PRESETS
        .iter()
        .find(|preset| preset.provider == provider)
        .and_then(|preset| preset.connection(api))
}

#[cfg(test)]
mod tests {
    use super::{PROVIDER_PRESETS, connection_template};
    use crate::{ApiProtocol, Provider};

    #[test]
    fn provider_presets_expose_only_their_supported_apis() {
        let nvidia = connection_template(Provider::NvidiaNim, ApiProtocol::OpenAiChatCompletions);
        let anthropic = connection_template(Provider::Anthropic, ApiProtocol::AnthropicMessages);

        assert!(nvidia.is_some());
        assert!(anthropic.is_some());
        assert!(connection_template(Provider::NvidiaNim, ApiProtocol::AnthropicMessages).is_none());
    }

    #[test]
    fn every_provider_default_is_one_of_its_supported_protocols() {
        assert!(
            PROVIDER_PRESETS
                .iter()
                .all(|preset| preset.apis.contains(&preset.default_api))
        );
    }

    #[test]
    fn custom_api_preset_uses_every_protocol_and_requires_a_base_url() {
        let preset = PROVIDER_PRESETS
            .iter()
            .find(|preset| preset.provider == Provider::CustomEndpoint)
            .unwrap();

        assert_eq!(preset.apis, ApiProtocol::ALL);
        assert!(preset.base_url_required);
        assert!(!preset.api_key_required);
    }
}
