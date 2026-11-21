use crate::{
    ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatStream, Provider, ProviderError,
    TokenUsage,
    provider::adapter::{AdapterSpec, add_header, endpoint_url, json_headers, start_stream},
    transport::http::{HttpRequest, HttpTransport, safe_excerpt},
    transport::jsonl::JsonlParser,
    transport::stream::DecodedFrame,
};
use reqwest::{Url, header};
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
    let endpoint = ollama_endpoint(&profile);
    start_stream(
        transport,
        profile,
        api_key,
        messages,
        cancellation,
        AdapterSpec {
            endpoint,
            build_request,
            parser: JsonlParser::new(),
            decode: decode_frame,
        },
    )
}

fn ollama_endpoint(profile: &AssistantProfile) -> Result<Url, String> {
    if profile.provider != Provider::Ollama || profile.api != ApiProtocol::OllamaChat {
        return Err("the Ollama Chat API requires an Ollama profile".to_owned());
    }
    endpoint_url(
        profile
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:11434"),
        "/api/chat",
    )
}

#[derive(Serialize)]
struct OllamaBody<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    stream: bool,
}

fn build_request(
    profile: &AssistantProfile,
    messages: Vec<ChatMessage>,
    api_key: Option<&str>,
    endpoint: Url,
) -> Result<HttpRequest, String> {
    let mut headers = json_headers("application/x-ndjson");
    if let Some(api_key) = api_key {
        add_header(
            &mut headers,
            header::AUTHORIZATION,
            &format!("Bearer {api_key}"),
        )?;
    }

    let body = serde_json::to_vec(&OllamaBody {
        model: &profile.model,
        messages,
        stream: true,
    })
    .map_err(|_| "could not encode the Ollama Chat request".to_owned())?;
    Ok(HttpRequest {
        method: reqwest::Method::POST,
        endpoint,
        headers,
        body: Some(body),
    })
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
    if let Some(message) = value.get("error").and_then(Value::as_str) {
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

    let done = value.get("done").and_then(Value::as_bool) == Some(true);
    let mut events = Vec::new();
    if let Some(text) = value.pointer("/message/content").and_then(Value::as_str)
        && !text.is_empty()
    {
        events.push(ChatEvent::TextDelta(text.to_owned()));
    }
    if done {
        let usage = TokenUsage {
            input_tokens: value.get("prompt_eval_count").and_then(Value::as_u64),
            output_tokens: value.get("eval_count").and_then(Value::as_u64),
            total_tokens: None,
        };

        let usage = TokenUsage {
            total_tokens: usage
                .input_tokens
                .zip(usage.output_tokens)
                .map(|(input, output)| input.saturating_add(output)),
            ..usage
        };
        if usage.input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.total_tokens.is_some()
        {
            events.push(ChatEvent::Usage(usage));
        }
    }
    Ok(if done {
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

    fn profile(api_key_env: Option<&str>) -> AssistantProfile {
        AssistantProfile {
            provider: Provider::Ollama,
            api: ApiProtocol::OllamaChat,
            model: "qwen3:8b".to_owned(),
            base_url: None,
            api_key_env: api_key_env.map(str::to_owned),
            credential_source: CredentialSource::Environment,
        }
    }

    #[tokio::test]
    async fn streams_jsonl_without_requiring_a_credential() {
        let (transport, captured) = FakeTransport::new(
            200,
            &[
                "{\"message\":{\"content\":\"Hel",
                "lo\"},\"done\":false}\r\n{\"message\":{\"content\":\"!\"},\"done\":true,\"prompt_eval_count\":5,\"eval_count\":2}",
            ],
        );
        let mut events = stream(
            transport,
            profile(None),
            None,
            vec![ChatMessage::new(ChatRole::User, "hi")],
            CancellationToken::new(),
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::TextDelta("Hello".to_owned())
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::TextDelta("!".to_owned())
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::Usage(TokenUsage {
                input_tokens: Some(5),
                output_tokens: Some(2),
                total_tokens: Some(7),
            })
        );
        assert!(events.next().await.is_none());
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(request.endpoint, "http://localhost:11434/api/chat");
        assert!(request.header("authorization").is_none());
        assert_eq!(request.header("accept"), Some("application/x-ndjson"));
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], "qwen3:8b");
        assert_eq!(body["stream"], true);
    }

    #[tokio::test]
    async fn optional_remote_ollama_credentials_are_redacted_from_errors() {
        let (transport, captured) =
            FakeTransport::new(200, &["{\"error\":\"bad key test-secret\"}\n"]);
        let mut events = stream(
            transport,
            profile(Some("OLLAMA_API_KEY")),
            Some("test-secret".to_owned()),
            vec![ChatMessage::new(ChatRole::User, "hello")],
            CancellationToken::new(),
        );
        let error = events.next().await.unwrap().unwrap_err().to_string();
        assert!(error.contains("bad key [redacted]"));
        assert!(!error.contains("test-secret"));
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(request.header("authorization"), Some("Bearer test-secret"));
    }
}
