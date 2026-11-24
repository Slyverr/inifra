use crate::{
    ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatStream, ProviderError, TokenUsage,
    provider::adapter::{AdapterSpec, bearer_headers, endpoint_url, start_stream},
    transport::http::{HttpRequest, HttpTransport, safe_excerpt},
    transport::sse::SseParser,
    transport::stream::DecodedFrame,
};
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
use crate::Provider;

pub(crate) fn stream_with_transport<T: HttpTransport + 'static>(
    transport: T,
    profile: AssistantProfile,
    api_key: Option<String>,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
) -> ChatStream {
    let endpoint = chat_completions_endpoint(&profile);
    start_stream(
        transport,
        profile,
        api_key,
        messages,
        cancellation,
        AdapterSpec {
            endpoint,
            build_request,
            parser: SseParser::new(),
            decode: decode_frame,
        },
    )
}

fn decode_frame(
    event: &str,
    profile: &AssistantProfile,
    endpoint: &Url,
    request_id: Option<&str>,
    api_key: Option<&str>,
) -> Result<DecodedFrame, ProviderError> {
    if event == "[DONE]" {
        return Ok(DecodedFrame::done(Vec::new()));
    }
    parse_event(event, profile, endpoint, request_id, api_key.unwrap_or(""))
        .map(DecodedFrame::events)
}

#[derive(Serialize)]
struct ChatCompletionsBody<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

fn build_request(
    profile: &AssistantProfile,
    messages: Vec<ChatMessage>,
    api_key: Option<&str>,
    endpoint: Url,
) -> Result<HttpRequest, String> {
    let headers = bearer_headers(api_key, "text/event-stream")?;
    let body = serde_json::to_vec(&ChatCompletionsBody {
        model: &profile.model,
        messages,
        stream: true,
        stream_options: profile.stream_usage_enabled().then_some(StreamOptions {
            include_usage: true,
        }),
        reasoning_effort: profile.effort.map(|effort| effort.as_str()),
    })
    .map_err(|_| "could not encode the chat request".to_owned())?;
    Ok(HttpRequest {
        method: reqwest::Method::POST,
        endpoint,
        headers,
        body: Some(body),
    })
}

fn chat_completions_endpoint(profile: &AssistantProfile) -> Result<Url, String> {
    if profile.api != ApiProtocol::OpenAiChatCompletions {
        return Err(format!(
            "{} does not use the Chat Completions API",
            profile.provider.display_name()
        ));
    }

    let base_url = profile
        .effective_base_url()
        .ok_or_else(|| "this API profile requires a base URL".to_owned())?;
    endpoint_url(base_url, "/chat/completions")
}

fn parse_event(
    event: &str,
    profile: &AssistantProfile,
    endpoint: &Url,
    request_id: Option<&str>,
    api_key: &str,
) -> Result<Vec<ChatEvent>, ProviderError> {
    let value: Value = serde_json::from_str(event).map_err(|_| {
        ProviderError::stream(
            profile.provider,
            profile.api,
            &profile.model,
            endpoint,
            request_id.map(str::to_owned),
            format!(
                "provider returned invalid streaming JSON: {}",
                safe_excerpt(event.as_bytes(), Some(api_key))
            ),
            Some(api_key),
        )
    })?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .map(|message| safe_excerpt(message.as_bytes(), Some(api_key)))
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| safe_excerpt(event.as_bytes(), Some(api_key)));
        return Err(ProviderError::stream(
            profile.provider,
            profile.api,
            &profile.model,
            endpoint,
            request_id.map(str::to_owned),
            message,
            Some(api_key),
        ));
    }

    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        let usage = TokenUsage {
            input_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
            output_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
            total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
        };
        if usage.input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.total_tokens.is_some()
        {
            events.push(ChatEvent::Usage(usage));
        }
    }
    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
        for choice in choices {
            if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str)
                && !text.is_empty()
            {
                events.push(ChatEvent::TextDelta(text.to_owned()));
            }
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::{ChatEvent, ChatMessage, Provider, stream_with_transport};
    use crate::{
        ApiProtocol, AssistantProfile, ChatRole, CredentialSource, TokenUsage,
        test_support::FakeTransport,
    };
    use futures_util::StreamExt;
    use tokio_util::sync::CancellationToken;

    fn profile(provider: Provider, base_url: Option<&str>) -> AssistantProfile {
        AssistantProfile {
            provider,
            api: ApiProtocol::OpenAiChatCompletions,
            model: "example/model".to_owned(),
            base_url: base_url.map(str::to_owned),
            api_key_env: Some("EXAMPLE_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        }
    }

    #[tokio::test]
    async fn streams_text_and_usage_from_split_sse_chunks() {
        let (transport, captured) = FakeTransport::new(
            200,
            &[
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hel",
                "lo\"}}]}\r\n\r\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":3,\"total_tokens\":12}}\n\ndata: [DONE]\n\n",
            ],
        );
        let mut events = stream_with_transport(
            transport,
            profile(Provider::NvidiaNim, None),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let mut received = Vec::new();
        while let Some(event) = events.next().await {
            received.push(event.unwrap());
        }
        assert_eq!(
            received,
            [
                ChatEvent::TextDelta("Hello".to_owned()),
                ChatEvent::Usage(TokenUsage {
                    input_tokens: Some(9),
                    output_tokens: Some(3),
                    total_tokens: Some(12),
                }),
            ]
        );
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            request.endpoint,
            "https://integrate.api.nvidia.com/v1/chat/completions"
        );
        assert_eq!(request.header("authorization"), Some("Bearer test-secret"));
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], "example/model");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[tokio::test]
    async fn appends_endpoint_once_and_preserves_a_custom_api_prefix() {
        let (transport, captured) = FakeTransport::new(200, &["data: [DONE]\n\n"]);
        let mut events = stream_with_transport(
            transport,
            profile(
                Provider::OpenAiCompatible,
                Some("https://llm.example/gateway/v1/chat/completions/"),
            ),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        assert!(events.next().await.is_none());
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            request.endpoint,
            "https://llm.example/gateway/v1/chat/completions"
        );
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert!(body.get("stream_options").is_none());
    }

    #[tokio::test]
    async fn http_errors_include_endpoint_status_request_id_and_redacted_detail() {
        let (transport, _) = FakeTransport::new(
            404,
            &[r#"{"error":{"message":"missing route for test-secret"}}"#],
        );
        let mut events = stream_with_transport(
            transport,
            profile(Provider::OpenAi, None),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err().to_string();
        assert!(error.contains("HTTP status 404"));
        assert!(error.contains("https://api.openai.com/[path redacted]/chat/completions"));
        assert!(error.contains("req-example-1"));
        assert!(error.contains("missing route for [redacted]"));
        assert!(!error.contains("test-secret"));
    }

    #[tokio::test]
    async fn invalid_endpoint_credentials_and_query_are_not_shown_in_errors() {
        let (transport, captured) = FakeTransport::new(200, &["data: [DONE]\n\n"]);
        let mut events = stream_with_transport(
            transport,
            profile(
                Provider::OpenAiCompatible,
                Some(
                    "https://user:urlsecret@llm.example/pathsecret/test-secret/v1?token=querysecret",
                ),
            ),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err().to_string();
        assert!(error.contains("https://llm.example/[path redacted]/chat/completions"));
        assert!(!error.contains("urlsecret"));
        assert!(!error.contains("pathsecret"));
        assert!(!error.contains("test-secret"));
        assert!(!error.contains("querysecret"));
        assert!(captured.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_pending_response_body() {
        let (mut transport, _) = FakeTransport::new(200, &[]);
        transport.pending_body = true;
        let cancellation = CancellationToken::new();
        let cancel_request = cancellation.clone();
        let mut events = stream_with_transport(
            transport,
            profile(Provider::OpenAi, None),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            cancellation,
        );
        let task = tokio::spawn(async move { events.next().await });
        tokio::task::yield_now().await;
        cancel_request.cancel();
        assert_eq!(task.await.unwrap(), Some(Ok(ChatEvent::Cancelled)));
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_pending_error_response_body() {
        let (mut transport, _) = FakeTransport::new(404, &[]);
        transport.pending_body = true;
        let cancellation = CancellationToken::new();
        let cancel_request = cancellation.clone();
        let mut events = stream_with_transport(
            transport,
            profile(Provider::OpenAi, None),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            cancellation,
        );
        let task = tokio::spawn(async move { events.next().await });
        tokio::task::yield_now().await;
        cancel_request.cancel();
        assert_eq!(task.await.unwrap(), Some(Ok(ChatEvent::Cancelled)));
    }

    #[tokio::test]
    async fn http_errors_redact_keys_echoed_in_request_ids() {
        let (mut transport, _) =
            FakeTransport::new(404, &[r#"{"error":{"message":"missing route"}}"#]);
        transport.request_id = Some("test-secret".to_owned());
        let mut events = stream_with_transport(
            transport,
            profile(Provider::OpenAi, None),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err().to_string();
        assert!(error.contains("request ID [redacted]"));
        assert!(!error.contains("test-secret"));
    }

    #[tokio::test]
    async fn stream_errors_redact_keys_echoed_in_request_ids() {
        let (mut transport, _) = FakeTransport::new(
            200,
            &[r#"data: {"error":{"message":"stream failed"}}"#, "\n\n"],
        );
        transport.request_id = Some("test-secret".to_owned());
        let mut events = stream_with_transport(
            transport,
            profile(Provider::OpenAi, None),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err().to_string();
        assert!(error.contains("request ID [redacted]"));
        assert!(!error.contains("test-secret"));
    }

    #[tokio::test]
    async fn missing_credentials_fail_before_sending_a_request() {
        let (transport, captured) = FakeTransport::new(200, &["data: [DONE]\n\n"]);
        let mut events = stream_with_transport(
            transport,
            profile(Provider::OpenAi, None),
            None,
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err();
        assert!(error.to_string().contains("API key is missing"));
        assert!(captured.lock().unwrap().is_none());
    }

    #[test]
    fn effort_maps_to_the_protocol_reasoning_setting_only_when_set() {
        let mut profile = crate::AssistantProfile {
            provider: crate::Provider::OpenRouter,
            api: crate::ApiProtocol::OpenAiChatCompletions,
            model: "model".to_owned(),
            base_url: None,
            api_key_env: None,
            credential_source: crate::CredentialSource::Environment,
            effort: Some(crate::Effort::High),
        };
        let messages = vec![crate::ChatMessage::new(crate::ChatRole::User, "hi")];
        let endpoint = reqwest::Url::parse("https://api.example.test/v1/chat/completions").unwrap();
        let body_of = |profile: &crate::AssistantProfile| -> serde_json::Value {
            let request =
                super::build_request(profile, messages.clone(), None, endpoint.clone()).unwrap();
            serde_json::from_slice(&request.body.unwrap()).unwrap()
        };

        let body = body_of(&profile);
        assert_eq!(body["reasoning_effort"], "high");

        profile.effort = None;
        let body = body_of(&profile);
        assert!(body.get("reasoning_effort").is_none());
    }
}
