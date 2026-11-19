//! Assistant profiles, provider adapters, and constrained assistant workflows.
#![deny(missing_docs)]

mod config;
mod credentials;

pub use config::{ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Provider};
pub use credentials::{read_credential, resolve_credential, write_credential};

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
