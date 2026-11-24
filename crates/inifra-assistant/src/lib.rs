//! Assistant profiles, provider adapters, and constrained assistant workflows.
#![deny(missing_docs)]

mod conversation;
mod provider;
mod settings;
#[cfg(test)]
mod test_support;
mod transport;

pub use conversation::chat::{ChatEvent, ChatMessage, ChatRole, ChatStream, TokenUsage};
pub use conversation::client::ProviderClient;
pub use conversation::models::list_models;
pub use settings::config::{
    ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Provider, is_valid_profile_id,
};
pub use settings::credentials::{read_credential, resolve_credential, write_credential};
pub use settings::presets::{
    ConnectionTemplate, PROVIDER_PRESETS, ProviderPreset, connection_template,
};
/// Cancellation signal used to stop a provider request and response stream.
pub use tokio_util::sync::CancellationToken;
pub use transport::http::{FailureKind, ProviderError};

/// An invalid assistant configuration or unavailable credential.
#[derive(Debug)]
pub struct AssistantError(String);

impl std::fmt::Display for AssistantError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AssistantError {}

impl From<std::io::Error> for AssistantError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

type Result<T> = std::result::Result<T, AssistantError>;

fn error(message: impl Into<String>) -> AssistantError {
    AssistantError(message.into())
}
