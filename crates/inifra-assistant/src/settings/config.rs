use crate::{AssistantError, Result, error};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};
use tempfile::NamedTempFile;
use url::Url;

const SCHEMA_VERSION: u32 = 1;

/// Provider protocol selected for an assistant profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApiProtocol {
    /// OpenAI Responses API.
    OpenAiResponses,
    /// OpenAI Chat Completions API and compatible services.
    OpenAiChatCompletions,
    /// Anthropic Messages API.
    AnthropicMessages,
    /// Google Gemini Generate Content API.
    GeminiGenerateContent,
    /// Ollama Chat API.
    OllamaChat,
}

impl ApiProtocol {
    /// Every API protocol implemented by the assistant adapters.
    pub const ALL: [Self; 5] = [
        Self::OpenAiResponses,
        Self::OpenAiChatCompletions,
        Self::AnthropicMessages,
        Self::GeminiGenerateContent,
        Self::OllamaChat,
    ];

    /// Short name used when the surrounding UI already identifies these as APIs.
    pub fn short_name(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "Responses",
            Self::OpenAiChatCompletions => "Chat Completions",
            Self::AnthropicMessages => "Messages",
            Self::GeminiGenerateContent => "Generate Content",
            Self::OllamaChat => "Ollama Chat",
        }
    }

    /// Human-readable API protocol name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "Responses API",
            Self::OpenAiChatCompletions => "Chat Completions API",
            Self::AnthropicMessages => "Messages API",
            Self::GeminiGenerateContent => "Generate Content API",
            Self::OllamaChat => "Ollama Chat API",
        }
    }
}

/// Service that implements an assistant API protocol.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    /// OpenAI hosted APIs.
    OpenAi,
    /// A user-managed OpenAI-compatible service using Chat Completions.
    OpenAiCompatible,
    /// A user-managed endpoint that implements one of the supported API protocols.
    CustomEndpoint,
    /// NVIDIA NIM.
    NvidiaNim,
    /// Anthropic hosted APIs.
    Anthropic,
    /// Google Gemini.
    Gemini,
    /// A local or remote Ollama server.
    Ollama,
    /// OpenRouter.
    OpenRouter,
}

impl Provider {
    /// Human-readable provider name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::OpenAiCompatible => "OpenAI-compatible",
            Self::CustomEndpoint => "Custom API",
            Self::NvidiaNim => "NVIDIA NIM",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Google Gemini",
            Self::Ollama => "Ollama",
            Self::OpenRouter => "OpenRouter",
        }
    }
}

/// Where a profile resolves its API credential at runtime.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialSource {
    /// Read the named credential from the process environment.
    #[default]
    Environment,
    /// Read the named credential from Inifra's literal `credentials.env` file.
    CredentialsFile,
}

/// Provider preset, API protocol, model, endpoint override, and credential reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantProfile {
    /// Service implementing the selected API.
    pub provider: Provider,
    /// Wire protocol used by the service.
    pub api: ApiProtocol,
    /// Model identifier supplied by the service.
    pub model: String,
    /// Optional API base URL. API paths are added by the provider adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Environment variable name used to look up this profile's API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Credential lookup source for the named API key.
    #[serde(default)]
    pub credential_source: CredentialSource,
}

impl AssistantProfile {
    /// Return the explicit endpoint override or the selected provider preset's default.
    pub fn effective_base_url(&self) -> Option<&str> {
        self.base_url.as_deref().or_else(|| {
            crate::connection_template(self.provider, self.api)
                .and_then(|template| template.base_url)
        })
    }

    /// Whether the selected provider preset enables Chat Completions stream usage metadata.
    pub fn stream_usage_enabled(&self) -> bool {
        crate::connection_template(self.provider, self.api)
            .is_some_and(|template| template.stream_usage)
    }

    /// Whether a profile using this provider and protocol requires an API-key variable.
    pub fn api_key_required(&self) -> bool {
        crate::connection_template(self.provider, self.api)
            .is_some_and(|template| template.api_key_required)
    }

    /// Validates that the service, API protocol, model, endpoint, and key reference agree.
    pub fn validate(&self) -> Result<()> {
        if self.model.trim().is_empty() || self.model.chars().any(char::is_control) {
            return Err(error(
                "assistant model must be a non-empty, single-line value",
            ));
        }

        let Some(template) = crate::connection_template(self.provider, self.api) else {
            return Err(error(format!(
                "API protocol {:?} is not supported by provider {:?}",
                self.api, self.provider
            )));
        };
        if template.api_key_required && self.api_key_env.is_none() {
            return Err(error(
                "this provider requires api_key_env to name its credential environment variable",
            ));
        }
        if self.credential_source == CredentialSource::CredentialsFile && self.api_key_env.is_none()
        {
            return Err(error(
                "credentials-file mode requires api_key_env to name the stored credential",
            ));
        }
        if let Some(name) = &self.api_key_env
            && !valid_env_name(name)
        {
            return Err(error(
                "api_key_env must be an uppercase environment variable name",
            ));
        }
        if template.base_url_required && self.base_url.is_none() {
            return Err(error("custom API services require a base_url"));
        }
        if let Some(base_url) = &self.base_url {
            let url = Url::parse(base_url).map_err(|_| error("base_url must be a valid URL"))?;
            let authority_has_userinfo = base_url
                .split_once("://")
                .and_then(|(_, remainder)| remainder.split(['/', '?', '#']).next())
                .is_some_and(|authority| authority.contains('@'));
            if !matches!(url.scheme(), "https" | "http")
                || url.host_str().is_none()
                || authority_has_userinfo
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(error(
                    "base_url must use http(s), include a host, and contain no credentials, query, or fragment",
                ));
            }
        }
        Ok(())
    }
}

/// Named assistant profiles and the profile selected by default.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantConfig {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_profile: Option<String>,
    #[serde(default)]
    profiles: BTreeMap<String, AssistantProfile>,
}

impl AssistantConfig {
    /// Loads assistant configuration, returning `None` when the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error.into());
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(error(format!(
                "assistant config must be a regular file: {}",
                path.display()
            )));
        }

        let contents = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)
            .map_err(|parse| error(format!("{}: {parse}", path.display())))?;
        config.validate()?;
        Ok(Some(config))
    }

    /// Writes the validated configuration atomically.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| error("assistant config path must have a parent directory"))?;
        fs::create_dir_all(parent)?;
        let parent_metadata = fs::symlink_metadata(parent)?;
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err(error(format!(
                "assistant config directory must be a real directory: {}",
                parent.display()
            )));
        }

        let text = toml::to_string_pretty(self).map_err(|parse| error(parse.to_string()))?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        std::io::Write::write_all(&mut temporary, text.as_bytes())?;
        temporary
            .persist(path)
            .map_err(|persist| AssistantError(persist.error.to_string()))?;
        Ok(())
    }

    /// Returns all profiles in stable ID order.
    pub fn profiles(&self) -> &BTreeMap<String, AssistantProfile> {
        &self.profiles
    }

    /// Returns a profile by ID.
    pub fn profile(&self, id: &str) -> Option<&AssistantProfile> {
        self.profiles.get(id)
    }

    /// Returns the configured default profile ID, if one exists.
    pub fn default_profile_id(&self) -> Option<&str> {
        self.default_profile.as_deref()
    }

    /// Returns the configured default profile, if one exists.
    pub fn default_profile(&self) -> Option<&AssistantProfile> {
        self.default_profile
            .as_deref()
            .and_then(|id| self.profile(id))
    }

    /// Adds a profile and selects it when it is the first profile.
    pub fn insert_profile(&mut self, id: &str, profile: AssistantProfile) -> Result<()> {
        validate_profile_id(id)?;
        profile.validate()?;
        if self.profiles.contains_key(id) {
            return Err(error(format!("assistant profile {id:?} already exists")));
        }
        self.profiles.insert(id.to_owned(), profile);
        self.default_profile.get_or_insert_with(|| id.to_owned());
        self.schema_version = SCHEMA_VERSION;
        Ok(())
    }

    /// Replaces an existing profile without changing its ID or default status.
    pub fn replace_profile(&mut self, id: &str, profile: AssistantProfile) -> Result<()> {
        validate_profile_id(id)?;
        profile.validate()?;
        let Some(existing) = self.profiles.get_mut(id) else {
            return Err(error(format!("unknown assistant profile {id:?}")));
        };
        *existing = profile;
        Ok(())
    }

    /// Selects the default profile.
    pub fn set_default_profile(&mut self, id: &str) -> Result<()> {
        if !self.profiles.contains_key(id) {
            return Err(error(format!("unknown assistant profile {id:?}")));
        }
        self.default_profile = Some(id.to_owned());
        Ok(())
    }

    /// Removes a profile and selects another remaining profile when the default was removed.
    pub fn remove_profile(&mut self, id: &str) -> Result<AssistantProfile> {
        let profile = self
            .profiles
            .remove(id)
            .ok_or_else(|| error(format!("unknown assistant profile {id:?}")))?;
        if self.default_profile.as_deref() == Some(id) {
            self.default_profile = self.profiles.keys().next().cloned();
        }
        Ok(profile)
    }

    /// Validates the schema, profile IDs, provider settings, and default selection.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(error(format!(
                "unsupported assistant config schema version {}",
                self.schema_version
            )));
        }

        for (id, profile) in &self.profiles {
            validate_profile_id(id)?;
            profile
                .validate()
                .map_err(|reason| error(format!("profile {id:?}: {reason}")))?;
        }
        match (&self.default_profile, self.profiles.is_empty()) {
            (Some(id), false) if self.profiles.contains_key(id) => Ok(()),
            (None, true) => Ok(()),
            (Some(id), _) => Err(error(format!(
                "default assistant profile {id:?} is not configured"
            ))),
            (None, false) => Err(error(
                "assistant config has profiles but no default profile",
            )),
        }
    }
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            default_profile: None,
            profiles: BTreeMap::new(),
        }
    }
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

/// Returns whether `id` is a valid profile ID: 1–64 lowercase ASCII letters, digits, or hyphens,
/// starting with a letter.
pub fn is_valid_profile_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && id.as_bytes()[0].is_ascii_lowercase()
}

fn validate_profile_id(id: &str) -> Result<()> {
    if is_valid_profile_id(id) {
        Ok(())
    } else {
        Err(error(format!("invalid assistant profile ID {id:?}")))
    }
}

pub(crate) fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}
