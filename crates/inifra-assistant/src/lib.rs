//! Assistant profiles, provider adapters, and constrained assistant workflows.
#![deny(missing_docs)]

mod adapter;
mod anthropic;
mod chat;
mod chat_completions;
mod client;
mod config;
mod credentials;
mod gemini;
mod http;
mod jsonl;
mod models;
mod ollama;
mod responses;
mod sse;
mod stream;
#[cfg(test)]
mod test_support;

pub use chat::{ChatEvent, ChatMessage, ChatRole, ChatStream, TokenUsage};
pub use client::ProviderClient;
pub use config::{ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Provider};
pub use credentials::{read_credential, resolve_credential, write_credential};
pub use http::ProviderError;
pub use models::list_models;
/// Cancellation signal used to stop a provider request and response stream.
pub use tokio_util::sync::CancellationToken;

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
