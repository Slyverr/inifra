use crate::ProviderError;
use futures_util::stream::BoxStream;
use serde::Serialize;

/// Role of a message sent to a text chat API.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// Instruction that applies to the whole conversation.
    System,
    /// Message written by the user.
    User,
    /// Message returned by the assistant.
    Assistant,
}

/// A text message in a chat request.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ChatMessage {
    /// Message role.
    pub role: ChatRole,
    /// Message text.
    pub content: String,
}

impl ChatMessage {
    /// Creates a message from a role and text.
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// Information produced while a provider response is streaming.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatEvent {
    /// A newly received piece of assistant text.
    TextDelta(String),
    /// Token counts reported by the provider; fields may arrive in separate events.
    Usage(TokenUsage),
    /// The request was cancelled by the caller.
    Cancelled,
}

/// Token counts reported by a provider, when available.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenUsage {
    /// Number of prompt or input tokens.
    pub input_tokens: Option<u64>,
    /// Number of generated or output tokens.
    pub output_tokens: Option<u64>,
    /// Total number of tokens.
    pub total_tokens: Option<u64>,
}

/// A stream of provider events or one normalized provider error.
pub type ChatStream = BoxStream<'static, Result<ChatEvent, ProviderError>>;
