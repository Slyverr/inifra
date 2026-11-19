use crate::{
    ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatStream, Provider, ProviderError,
    TokenUsage,
    adapter::{AdapterSpec, bearer_headers, endpoint_url, start_stream},
    http::{HttpRequest, HttpTransport, safe_excerpt},
    sse::SseParser,
    stream::DecodedFrame,
};
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub(crate) fn stream<T: HttpTransport + 'static>(
    transport: T,
    profile: AssistantProfile,
    api_key: Option<String>,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
) -> ChatStream {
    let endpoint = responses_endpoint(&profile);
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

fn responses_endpoint(profile: &AssistantProfile) -> Result<Url, String> {
    if profile.provider != Provider::OpenAi || profile.api != ApiProtocol::OpenAiResponses {
        return Err("the Responses API is currently supported for OpenAI profiles".to_owned());
    }
    endpoint_url(
        profile
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1"),
        "/responses",
    )
}

#[derive(Serialize)]
struct ResponsesBody<'a> {
    model: &'a str,
    input: Vec<ChatMessage>,
    stream: bool,
}

fn build_request(
    profile: &AssistantProfile,
    messages: Vec<ChatMessage>,
    api_key: Option<&str>,
    endpoint: Url,
) -> Result<HttpRequest, String> {
    let headers = bearer_headers(api_key, "text/event-stream")?;
    let body = serde_json::to_vec(&ResponsesBody {
        model: &profile.model,
        input: messages,
        stream: true,
    })
    .map_err(|_| "could not encode the Responses API request".to_owned())?;
    Ok(HttpRequest {
        endpoint,
        headers,
        body,
    })
}

fn decode_frame(
    frame: &str,
    profile: &AssistantProfile,
    endpoint: &Url,
    request_id: Option<&str>,
    api_key: Option<&str>,
) -> Result<DecodedFrame, ProviderError> {
    if frame == "[DONE]" {
        return Ok(DecodedFrame::done(Vec::new()));
    }
    let api_key = api_key.unwrap_or_default();
    let value: Value = serde_json::from_str(frame).map_err(|_| {
        ProviderError::stream(
            profile.provider,
            profile.api,
            &profile.model,
            endpoint,
            request_id.map(str::to_owned),
            format!(
                "provider returned invalid streaming JSON: {}",
                safe_excerpt(frame.as_bytes(), Some(api_key))
            ),
            Some(api_key),
        )
    })?;
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(message) = error_message(&value) {
        return Err(ProviderError::stream(
            profile.provider,
            profile.api,
            &profile.model,
            endpoint,
            request_id.map(str::to_owned),
            safe_excerpt(message.as_bytes(), Some(api_key)),
            Some(api_key),
        ));
    }

    let mut events = Vec::new();
    if event_type == "response.output_text.delta"
        && let Some(text) = value.get("delta").and_then(Value::as_str)
        && !text.is_empty()
    {
        events.push(ChatEvent::TextDelta(text.to_owned()));
    }
    if matches!(
        event_type,
        "response.completed" | "response.incomplete" | "response.failed"
    ) && let Some(usage) = value.pointer("/response/usage")
    {
        let usage = TokenUsage {
            input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
            output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
            total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
        };
        if usage.input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.total_tokens.is_some()
        {
            events.push(ChatEvent::Usage(usage));
        }
    }
    let done = matches!(
        event_type,
        "response.completed" | "response.incomplete" | "response.failed"
    );
    Ok(if done {
        DecodedFrame::done(events)
    } else {
        DecodedFrame::events(events)
    })
}

fn error_message(value: &Value) -> Option<&str> {
    value
        .pointer("/error/message")
        .or_else(|| value.pointer("/response/error/message"))
        .and_then(Value::as_str)
        .or_else(|| {
            (value.get("type").and_then(Value::as_str) == Some("response.failed"))
                .then_some("provider reported that the response failed")
        })
}

#[cfg(test)]
mod tests {
    use super::stream;
    use crate::{
        ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatRole, CredentialSource,
        Provider, TokenUsage, test_support::FakeTransport,
    };
    use futures_util::StreamExt;
    use tokio_util::sync::CancellationToken;

    fn profile() -> AssistantProfile {
        AssistantProfile {
            provider: Provider::OpenAi,
            api: ApiProtocol::OpenAiResponses,
            model: "gpt-example".to_owned(),
            base_url: None,
            api_key_env: Some("OPENAI_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
        }
    }

    #[tokio::test]
    async fn streams_responses_text_and_usage_with_the_expected_request() {
        let (transport, captured) = FakeTransport::new(
            200,
            &[
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel",
                "lo\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":8,\"output_tokens\":2,\"total_tokens\":10}}}\n\n",
            ],
        );
        let mut events = stream(
            transport,
            profile(),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::TextDelta("Hello".to_owned())
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::Usage(TokenUsage {
                input_tokens: Some(8),
                output_tokens: Some(2),
                total_tokens: Some(10),
            })
        );
        assert!(events.next().await.is_none());
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(request.endpoint, "https://api.openai.com/v1/responses");
        assert_eq!(request.header("authorization"), Some("Bearer test-secret"));
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], "gpt-example");
        assert_eq!(body["stream"], true);
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["input"][0]["content"], "hello");
    }

    #[tokio::test]
    async fn response_errors_redact_the_configured_credential() {
        let (transport, _) = FakeTransport::new(
            200,
            &["data: {\"type\":\"error\",\"error\":{\"message\":\"rejected test-secret\"}}\n\n"],
        );
        let mut events = stream(
            transport,
            profile(),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err().to_string();
        assert!(error.contains("rejected [redacted]"));
        assert!(!error.contains("test-secret"));
    }
}
