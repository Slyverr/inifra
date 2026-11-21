use crate::{
    ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatRole, ChatStream, Provider,
    ProviderError, TokenUsage,
    provider::adapter::{AdapterSpec, add_header, json_headers, start_stream},
    transport::http::{HttpRequest, HttpTransport, safe_excerpt},
    transport::sse::SseParser,
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
    let endpoint = gemini_endpoint(&profile);
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

fn gemini_endpoint(profile: &AssistantProfile) -> Result<Url, String> {
    if profile.provider != Provider::Gemini || profile.api != ApiProtocol::GeminiGenerateContent {
        return Err("Generate Content API requires a Google Gemini profile".to_owned());
    }

    let base = profile
        .base_url
        .as_deref()
        .unwrap_or("https://generativelanguage.googleapis.com/v1beta");
    let mut url = Url::parse(base).map_err(|_| "API base URL is invalid".to_owned())?;
    let model = profile
        .model
        .strip_prefix("models/")
        .unwrap_or(&profile.model);
    let endpoint_suffix = format!("/models/{model}:streamGenerateContent");
    if !url.path().ends_with(&endpoint_suffix) {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| "API base URL cannot accept a model path".to_owned())?;
        path.pop_if_empty()
            .push("models")
            .push(&format!("{model}:streamGenerateContent"));
    }
    url.query_pairs_mut().append_pair("alt", "sse");
    url.set_fragment(None);
    Ok(url)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    contents: Vec<GeminiContent>,
}

#[derive(Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiContent {
    role: &'static str,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

fn build_request(
    _profile: &AssistantProfile,
    messages: Vec<ChatMessage>,
    api_key: Option<&str>,
    endpoint: Url,
) -> Result<HttpRequest, String> {
    let api_key = api_key.ok_or_else(|| "Google Gemini API key is missing".to_owned())?;
    let (system_instruction, contents) = gemini_messages(messages);
    if contents.is_empty() {
        return Err("Gemini chat requires at least one user or assistant message".to_owned());
    }

    let mut headers = json_headers("text/event-stream");
    add_header(
        &mut headers,
        header::HeaderName::from_static("x-goog-api-key"),
        api_key,
    )?;
    let body = serde_json::to_vec(&GeminiBody {
        system_instruction,
        contents,
    })
    .map_err(|_| "could not encode the Gemini Generate Content request".to_owned())?;
    Ok(HttpRequest {
        method: reqwest::Method::POST,
        endpoint,
        headers,
        body: Some(body),
    })
}

fn gemini_messages(
    messages: Vec<ChatMessage>,
) -> (Option<GeminiSystemInstruction>, Vec<GeminiContent>) {
    let mut system_text = Vec::new();
    let mut contents = Vec::new();
    for message in messages {
        match message.role {
            ChatRole::System => system_text.push(message.content),
            ChatRole::User => contents.push(content("user", message.content)),
            ChatRole::Assistant => contents.push(content("model", message.content)),
        }
    }

    let system_instruction = (!system_text.is_empty()).then(|| GeminiSystemInstruction {
        parts: vec![GeminiPart {
            text: system_text.join("\n\n"),
        }],
    });
    (system_instruction, contents)
}

fn content(role: &'static str, text: String) -> GeminiContent {
    GeminiContent {
        role,
        parts: vec![GeminiPart { text }],
    }
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
    if let Some(reason) = value
        .pointer("/promptFeedback/blockReason")
        .and_then(Value::as_str)
    {
        return Err(ProviderError::stream(
            profile.provider,
            profile.api,
            &profile.model,
            endpoint,
            request_id.map(str::to_owned),
            format!("Gemini blocked the prompt ({reason})"),
            Some(api_key),
        ));
    }

    let mut events = Vec::new();
    if let Some(parts) = value
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
    {
        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str)
                && !text.is_empty()
            {
                events.push(ChatEvent::TextDelta(text.to_owned()));
            }
        }
    }
    if let Some(usage) = value.get("usageMetadata") {
        let usage = TokenUsage {
            input_tokens: usage.get("promptTokenCount").and_then(Value::as_u64),
            output_tokens: usage.get("candidatesTokenCount").and_then(Value::as_u64),
            total_tokens: usage.get("totalTokenCount").and_then(Value::as_u64),
        };
        if usage.input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.total_tokens.is_some()
        {
            events.push(ChatEvent::Usage(usage));
        }
    }

    let finish_reason = value
        .pointer("/candidates/0/finishReason")
        .and_then(Value::as_str);
    let done = finish_reason.is_some_and(|reason| reason != "FINISH_REASON_UNSPECIFIED");
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

    fn profile() -> AssistantProfile {
        AssistantProfile {
            provider: Provider::Gemini,
            api: ApiProtocol::GeminiGenerateContent,
            model: "gemini-example".to_owned(),
            base_url: None,
            api_key_env: Some("GEMINI_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
        }
    }

    #[tokio::test]
    async fn streams_gemini_text_and_usage_with_key_in_header() {
        let (transport, captured) = FakeTransport::new(
            200,
            &[
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\ndata: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":6,\"candidatesTokenCount\":3,\"totalTokenCount\":9}}\n\n",
            ],
        );
        let mut events = stream(
            transport,
            profile(),
            Some("test-secret".to_owned()),
            vec![
                ChatMessage::new(ChatRole::System, "Be concise"),
                ChatMessage::new(ChatRole::Assistant, "Earlier"),
                ChatMessage::new(ChatRole::User, "hello"),
            ],
            CancellationToken::new(),
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::TextDelta("Hello".to_owned())
        );
        assert_eq!(
            events.next().await.unwrap().unwrap(),
            ChatEvent::Usage(TokenUsage {
                input_tokens: Some(6),
                output_tokens: Some(3),
                total_tokens: Some(9),
            })
        );
        assert!(events.next().await.is_none());
        let request = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            request.endpoint,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-example:streamGenerateContent?alt=sse"
        );
        assert_eq!(request.header("x-goog-api-key"), Some("test-secret"));
        assert!(request.header("authorization").is_none());
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "Be concise");
        assert_eq!(body["contents"][0]["role"], "model");
        assert_eq!(body["contents"][1]["role"], "user");
    }

    #[tokio::test]
    async fn gemini_errors_redact_keys_and_model_ids_from_the_endpoint() {
        let (transport, _) = FakeTransport::new(
            200,
            &["data: {\"error\":{\"message\":\"rejected test-secret\"}}\n\n"],
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
        assert!(error.contains("/[path redacted]/models/[model]:streamGenerateContent"));
        assert!(!error.contains("test-secret"));
        assert!(!error.contains("/models/gemini-example:"));
    }
}
