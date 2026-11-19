use crate::{
    ApiProtocol, AssistantProfile, ChatMessage, ChatStream, ProviderError,
    adapter::one_error,
    anthropic, chat_completions, gemini,
    http::{HttpTransport, ReqwestTransport},
    ollama, responses,
};
use tokio_util::sync::CancellationToken;

/// HTTP client for streaming assistant responses from configured providers.
#[derive(Clone)]
pub struct ProviderClient {
    transport: ReqwestTransport,
}

impl ProviderClient {
    /// Creates a provider client using the system's HTTPS trust roots.
    pub fn new() -> Result<Self, crate::AssistantError> {
        let transport = ReqwestTransport::new()
            .map_err(|error| crate::error(format!("could not initialize HTTPS client: {error}")))?;
        Ok(Self { transport })
    }

    /// Streams normalized text, usage, cancellation, and errors for a profile's configured API.
    ///
    /// The profile and conversation are owned by the returned stream. Cancelling the token stops
    /// both an in-progress request and response-body reads. Credentials are held only for the
    /// request and are never included in diagnostics.
    pub fn stream_chat(
        &self,
        profile: AssistantProfile,
        api_key: Option<String>,
        messages: Vec<ChatMessage>,
        cancellation: CancellationToken,
    ) -> ChatStream {
        stream_with_transport(
            self.transport.clone(),
            profile,
            api_key,
            messages,
            cancellation,
        )
    }

    /// Streams OpenAI Chat Completions-compatible responses.
    ///
    /// Use [`Self::stream_chat`] for profile-dispatched APIs.
    pub fn stream_chat_completions(
        &self,
        profile: AssistantProfile,
        api_key: Option<String>,
        messages: Vec<ChatMessage>,
        cancellation: CancellationToken,
    ) -> ChatStream {
        if profile.api != ApiProtocol::OpenAiChatCompletions {
            return one_error(ProviderError::configuration(
                profile.provider,
                profile.api,
                &profile.model,
                "profile does not use the Chat Completions API",
            ));
        }
        self.stream_chat(profile, api_key, messages, cancellation)
    }
}

fn stream_with_transport<T: HttpTransport + 'static>(
    transport: T,
    profile: AssistantProfile,
    api_key: Option<String>,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
) -> ChatStream {
    match profile.api {
        ApiProtocol::OpenAiResponses => {
            responses::stream(transport, profile, api_key, messages, cancellation)
        }
        ApiProtocol::OpenAiChatCompletions => chat_completions::stream_with_transport(
            transport,
            profile,
            api_key,
            messages,
            cancellation,
        ),
        ApiProtocol::AnthropicMessages => {
            anthropic::stream(transport, profile, api_key, messages, cancellation)
        }
        ApiProtocol::GeminiGenerateContent => {
            gemini::stream(transport, profile, api_key, messages, cancellation)
        }
        ApiProtocol::OllamaChat => {
            ollama::stream(transport, profile, api_key, messages, cancellation)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stream_with_transport;
    use crate::{
        ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatRole, CredentialSource,
        Provider, test_support::FakeTransport,
    };
    use futures_util::StreamExt;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn dispatches_profiles_to_the_selected_protocol_adapter() {
        let (transport, captured) = FakeTransport::new(
            200,
            &["data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n"],
        );
        let profile = AssistantProfile {
            provider: Provider::OpenAi,
            api: ApiProtocol::OpenAiResponses,
            model: "gpt-example".to_owned(),
            base_url: None,
            api_key_env: Some("OPENAI_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
        };
        let mut events = stream_with_transport(
            transport,
            profile,
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::TextDelta("ok".to_owned())
        );
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(request.endpoint, "https://api.openai.com/v1/responses");
    }
}
