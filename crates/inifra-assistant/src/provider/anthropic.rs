use crate::{
    ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatRole, ChatStream, ProviderError,
    TokenUsage,
    provider::adapter::{AdapterSpec, add_header, endpoint_url, json_headers, start_stream},
    transport::http::{HttpRequest, HttpTransport, safe_excerpt},
    transport::sse::SseParser,
    transport::stream::DecodedFrame,
};
use reqwest::{Url, header};
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;
const ANTHROPIC_API_VERSION: &str = "2026-10-21";

pub(crate) fn stream<T: HttpTransport + 'static>(
    transport: T,
    profile: AssistantProfile,
    api_key: Option<String>,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
) -> ChatStream {
    let endpoint = anthropic_endpoint(&profile);
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

fn anthropic_endpoint(profile: &AssistantProfile) -> Result<Url, String> {
    if profile.api != ApiProtocol::AnthropicMessages {
        return Err("the Messages API requires a Messages API profile".to_owned());
    }
    let base_url = profile
        .effective_base_url()
        .ok_or_else(|| "this API profile requires a base URL".to_owned())?;
    endpoint_url(base_url, "/messages")
}

#[derive(Serialize)]
struct Thinking {
    #[serde(rename = "type")]
    kind: &'static str,
    budget_tokens: u32,
}

#[derive(Serialize)]
struct AnthropicBody<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<Thinking>,
}

fn build_request(
    profile: &AssistantProfile,
    messages: Vec<ChatMessage>,
    api_key: Option<&str>,
    endpoint: Url,
) -> Result<HttpRequest, String> {
    let (system, messages) = anthropic_messages(messages);
    if messages.is_empty() {
        return Err("Anthropic chat requires at least one user or assistant message".to_owned());
    }

    let mut headers = json_headers("text/event-stream");
    if let Some(api_key) = api_key {
        add_header(
            &mut headers,
            header::HeaderName::from_static("x-api-key"),
            api_key,
        )?;
    }
    add_header(
        &mut headers,
        header::HeaderName::from_static("anthropic-version"),
        ANTHROPIC_API_VERSION,
    )?;
    let thinking = profile.effort.map(|effort| Thinking {
        kind: "enabled",
        budget_tokens: effort.thinking_budget(),
    });
    // The answer must fit after the reasoning budget, so extended thinking raises the limit.
    let max_tokens = DEFAULT_MAX_OUTPUT_TOKENS
        + thinking
            .as_ref()
            .map_or(0, |thinking| thinking.budget_tokens);
    let body = serde_json::to_vec(&AnthropicBody {
        model: &profile.model,
        max_tokens,
        system,
        messages,
        stream: true,
        thinking,
    })
    .map_err(|_| "could not encode the Anthropic Messages request".to_owned())?;
    Ok(HttpRequest {
        method: reqwest::Method::POST,
        endpoint,
        headers,
        body: Some(body),
    })
}

fn anthropic_messages(messages: Vec<ChatMessage>) -> (Option<String>, Vec<ChatMessage>) {
    let mut system = Vec::new();
    let mut conversation = Vec::new();
    for message in messages {
        if message.role == ChatRole::System {
            system.push(message.content);
        } else {
            conversation.push(message);
        }
    }

    let system = (!system.is_empty()).then(|| system.join("\n\n"));
    (system, conversation)
}

fn decode_frame(
    frame: &str,
    profile: &AssistantProfile,
    endpoint: &Url,
    request_id: Option<&str>,
    api_key: Option<&str>,
) -> Result<DecodedFrame, ProviderError> {
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
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
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
    if event_type == "error" {
        return Err(ProviderError::stream(
            profile.provider,
            profile.api,
            &profile.model,
            endpoint,
            request_id.map(str::to_owned),
            "Anthropic reported a streaming error",
            Some(api_key),
        ));
    }

    let mut events = Vec::new();
    if event_type == "content_block_delta"
        && value.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta")
        && let Some(text) = value.pointer("/delta/text").and_then(Value::as_str)
        && !text.is_empty()
    {
        events.push(ChatEvent::TextDelta(text.to_owned()));
    }

    let usage = match event_type {
        "message_start" => value.pointer("/message/usage").map(|usage| TokenUsage {
            input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
            output_tokens: None,
            total_tokens: None,
        }),
        "message_delta" => value.get("usage").map(|usage| TokenUsage {
            input_tokens: None,
            output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
            total_tokens: None,
        }),
        _ => None,
    };
    if let Some(usage) = usage
        && (usage.input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.total_tokens.is_some())
    {
        events.push(ChatEvent::Usage(usage));
    }
    Ok(if event_type == "message_stop" {
        DecodedFrame::done(events)
    } else {
        DecodedFrame::events(events)
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
            provider: Provider::Anthropic,
            api: ApiProtocol::AnthropicMessages,
            model: "claude-example".to_owned(),
            base_url: None,
            api_key_env: Some("ANTHROPIC_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        }
    }

    #[tokio::test]
    async fn streams_anthropic_text_and_usage_with_protocol_headers() {
        let (transport, captured) = FakeTransport::new(
            200,
            &[
                "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":7}}}\n\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
            ],
        );
        let mut events = stream(
            transport,
            profile(),
            Some("test-secret".to_owned()),
            vec![
                ChatMessage::new(ChatRole::System, "Be concise"),
                ChatMessage::new(ChatRole::User, "hello"),
            ],
            CancellationToken::new(),
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::Usage(TokenUsage {
                input_tokens: Some(7),
                output_tokens: None,
                total_tokens: None,
            })
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::TextDelta("Hello".to_owned())
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::Usage(TokenUsage {
                input_tokens: None,
                output_tokens: Some(2),
                total_tokens: None,
            })
        );
        assert!(events.next().await.is_none());
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(request.endpoint, "https://api.anthropic.com/v1/messages");
        assert_eq!(request.header("x-api-key"), Some("test-secret"));
        assert_eq!(request.header("anthropic-version"), Some("2026-10-21"));
        assert!(request.header("authorization").is_none());
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["system"], "Be concise");
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[tokio::test]
    async fn anthropic_stream_errors_redact_the_api_key() {
        let (transport, _) = FakeTransport::new(
            200,
            &["data: {\"type\":\"error\",\"error\":{\"message\":\"invalid test-secret\"}}\n\n"],
        );
        let mut events = stream(
            transport,
            profile(),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err().to_string();
        assert!(error.contains("invalid [redacted]"));
        assert!(!error.contains("test-secret"));
    }

    #[test]
    fn effort_maps_to_the_protocol_reasoning_setting_only_when_set() {
        let mut profile = crate::AssistantProfile {
            provider: crate::Provider::Anthropic,
            api: crate::ApiProtocol::AnthropicMessages,
            model: "model".to_owned(),
            base_url: None,
            api_key_env: None,
            credential_source: crate::CredentialSource::Environment,
            effort: Some(crate::Effort::High),
        };
        let messages = vec![crate::ChatMessage::new(crate::ChatRole::User, "hi")];
        let endpoint = reqwest::Url::parse("https://api.example.test/v1/messages").unwrap();
        let body_of = |profile: &crate::AssistantProfile| -> serde_json::Value {
            let request =
                super::build_request(profile, messages.clone(), None, endpoint.clone()).unwrap();
            serde_json::from_slice(&request.body.unwrap()).unwrap()
        };

        let body = body_of(&profile);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 24_576);
        assert!(body["max_tokens"].as_u64().unwrap() > 24_576);

        profile.effort = None;
        let body = body_of(&profile);
        assert!(body.get("thinking").is_none());
        assert_eq!(body["max_tokens"], 4096);
    }
}
