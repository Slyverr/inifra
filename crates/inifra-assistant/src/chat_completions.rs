use crate::{
    ApiProtocol, AssistantProfile, ChatEvent, ChatMessage, ChatStream, Provider, ProviderError,
    TokenUsage,
    http::{
        BodyStream, HttpRequest, HttpTransport, ReqwestTransport, TransportError, safe_excerpt,
    },
    sse::SseParser,
};
use futures_util::StreamExt;
use reqwest::{Url, header};
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

const MAX_ERROR_BODY_BYTES: usize = 16 * 1024;

/// HTTP client for streaming OpenAI Chat Completions-compatible responses.
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

    /// Streams text and usage events from OpenAI, NVIDIA NIM, OpenRouter, or a compatible endpoint.
    ///
    /// The profile and conversation are owned by the returned stream. Cancelling the token stops
    /// both an in-progress request and response-body reads. The API key is held only for the
    /// request and is never included in diagnostics.
    pub fn stream_chat_completions(
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
}

fn stream_with_transport<T: HttpTransport + 'static>(
    transport: T,
    profile: AssistantProfile,
    api_key: Option<String>,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
) -> ChatStream {
    Box::pin(async_stream::stream! {
        let endpoint = match chat_completions_endpoint(&profile) {
            Ok(endpoint) => endpoint,
            Err(message) => {
                yield Err(ProviderError::configuration(
                    profile.provider,
                    profile.api,
                    &profile.model,
                    message,
                ));
                return;
            }
        };
        if let Err(error) = profile.validate() {
            yield Err(ProviderError::request(
                profile.provider,
                profile.api,
                &profile.model,
                &endpoint,
                error.to_string(),
                None,
            ));
            return;
        }
        if messages.is_empty() {
            yield Err(ProviderError::request(
                profile.provider,
                profile.api,
                &profile.model,
                &endpoint,
                "chat request requires at least one message",
                None,
            ));
            return;
        }
        let Some(api_key) = api_key.filter(|key| !key.trim().is_empty()) else {
            yield Err(ProviderError::request(
                profile.provider,
                profile.api,
                &profile.model,
                &endpoint,
                "API key is missing; configure the profile credential",
                None,
            ));
            return;
        };
        let request = match build_request(&profile, &api_key, messages, endpoint.clone()) {
            Ok(request) => request,
            Err(message) => {
                yield Err(ProviderError::request(
                    profile.provider,
                    profile.api,
                    &profile.model,
                    &endpoint,
                    message,
                    Some(&api_key),
                ));
                return;
            }
        };
        let response = match transport.execute(request, &cancellation).await {
            Ok(response) => response,
            Err(TransportError::Cancelled) => {
                yield Ok(ChatEvent::Cancelled);
                return;
            }
            Err(TransportError::Network(message)) => {
                yield Err(ProviderError::request(
                    profile.provider,
                    profile.api,
                    &profile.model,
                    &endpoint,
                    safe_excerpt(message.as_bytes(), Some(&api_key)),
                    Some(&api_key),
                ));
                return;
            }
        };
        if !(200..300).contains(&response.status) {
            let (body, read_error) = match read_error_body(response.body, &cancellation).await {
                Ok(body) => body,
                Err(TransportError::Cancelled) => {
                    yield Ok(ChatEvent::Cancelled);
                    return;
                }
                Err(TransportError::Network(message)) => {
                    (Vec::new(), Some(message))
                }
            };
            let mut detail = error_message(&body, &api_key);
            if let Some(read_error) = read_error {
                if !detail.is_empty() {
                    detail.push_str("; ");
                }
                detail.push_str("could not read the error response: ");
                detail.push_str(&safe_excerpt(read_error.as_bytes(), Some(&api_key)));
            }
            yield Err(ProviderError::response(
                profile.provider,
                profile.api,
                &profile.model,
                &endpoint,
                response.status,
                response.request_id,
                detail,
                &api_key,
            ));
            return;
        }

        let mut body = response.body;
        let mut parser = SseParser::new();
        loop {
            let next = tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    yield Ok(ChatEvent::Cancelled);
                    return;
                }
                next = body.next() => next,
            };
            let Some(chunk) = next else {
                break;
            };
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(TransportError::Cancelled) => {
                    yield Ok(ChatEvent::Cancelled);
                    return;
                }
                Err(TransportError::Network(message)) => {
                    yield Err(ProviderError::stream(
                        profile.provider,
                        profile.api,
                        &profile.model,
                        &endpoint,
                        response.request_id,
                        safe_excerpt(message.as_bytes(), Some(&api_key)),
                        &api_key,
                    ));
                    return;
                }
            };
            let events = match parser.push(&chunk) {
                Ok(events) => events,
                Err(message) => {
                    yield Err(ProviderError::stream(
                        profile.provider,
                        profile.api,
                        &profile.model,
                        &endpoint,
                        response.request_id,
                        message,
                        &api_key,
                    ));
                    return;
                }
            };
            for event in events {
                if event == "[DONE]" {
                    return;
                }
                match parse_event(&event, &profile, &endpoint, response.request_id.as_deref(), &api_key) {
                    Ok(events) => {
                        for event in events {
                            yield Ok(event);
                        }
                    }
                    Err(error) => {
                        yield Err(error);
                        return;
                    }
                }
            }
        }
        let events = match parser.finish() {
            Ok(events) => events,
            Err(message) => {
                yield Err(ProviderError::stream(
                    profile.provider,
                    profile.api,
                    &profile.model,
                    &endpoint,
                    response.request_id,
                    message,
                    &api_key,
                ));
                return;
            }
        };
        for event in events {
            if event == "[DONE]" {
                return;
            }
            match parse_event(&event, &profile, &endpoint, response.request_id.as_deref(), &api_key) {
                Ok(events) => {
                    for event in events {
                        yield Ok(event);
                    }
                }
                Err(error) => {
                    yield Err(error);
                    return;
                }
            }
        }
    })
}

#[derive(Serialize)]
struct ChatCompletionsBody<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

fn build_request(
    profile: &AssistantProfile,
    api_key: &str,
    messages: Vec<ChatMessage>,
    endpoint: Url,
) -> Result<HttpRequest, String> {
    let mut headers = header::HeaderMap::new();
    let authorization = header::HeaderValue::from_str(&format!("Bearer {api_key}"))
        .map_err(|_| "API key cannot be represented as an HTTP header".to_owned())?;
    headers.insert(header::AUTHORIZATION, authorization);
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("text/event-stream"),
    );
    let body = serde_json::to_vec(&ChatCompletionsBody {
        model: &profile.model,
        messages,
        stream: true,
        stream_options: (profile.provider != Provider::OpenAiCompatible).then_some(StreamOptions {
            include_usage: true,
        }),
    })
    .map_err(|_| "could not encode the chat request".to_owned())?;
    Ok(HttpRequest {
        endpoint,
        headers,
        body,
    })
}

fn chat_completions_endpoint(profile: &AssistantProfile) -> Result<Url, String> {
    if profile.api != ApiProtocol::OpenAiChatCompletions {
        return Err(format!(
            "{} does not use the Chat Completions API",
            profile.provider.display_name()
        ));
    }
    let default_base = match profile.provider {
        Provider::OpenAi => "https://api.openai.com/v1",
        Provider::NvidiaNim => "https://integrate.api.nvidia.com/v1",
        Provider::OpenRouter => "https://openrouter.ai/api/v1",
        Provider::OpenAiCompatible => {
            return profile
                .base_url
                .as_deref()
                .ok_or_else(|| "custom endpoint requires an API base URL".to_owned())
                .and_then(append_chat_path);
        }
        _ => {
            return Err(format!(
                "{} does not use the Chat Completions API",
                profile.provider.display_name()
            ));
        }
    };
    append_chat_path(profile.base_url.as_deref().unwrap_or(default_base))
}

fn append_chat_path(base: &str) -> Result<Url, String> {
    let mut url = Url::parse(base).map_err(|_| "API base URL is invalid".to_owned())?;
    let path = url.path().trim_end_matches('/');
    let path = path
        .strip_suffix("/chat/completions")
        .unwrap_or(path)
        .trim_end_matches('/');
    url.set_path(&format!("{path}/chat/completions"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
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
            api_key,
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
            api_key,
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

async fn read_error_body(
    mut body: BodyStream,
    cancellation: &CancellationToken,
) -> Result<(Vec<u8>, Option<String>), TransportError> {
    let mut contents = Vec::new();
    loop {
        let next = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(TransportError::Cancelled),
            next = body.next() => next,
        };
        let Some(chunk) = next else {
            return Ok((contents, None));
        };
        let chunk = chunk?;
        let remaining = MAX_ERROR_BODY_BYTES.saturating_sub(contents.len());
        contents.extend(chunk.into_iter().take(remaining));
        if contents.len() == MAX_ERROR_BODY_BYTES {
            return Ok((contents, None));
        }
    }
}

fn error_message(bytes: &[u8], api_key: &str) -> String {
    let message = serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    message.map_or_else(
        || safe_excerpt(bytes, Some(api_key)),
        |message| safe_excerpt(message.as_bytes(), Some(api_key)),
    )
}

#[cfg(test)]
mod tests {
    use super::{ChatEvent, ChatMessage, Provider, stream_with_transport};
    use crate::{
        ApiProtocol, AssistantProfile, ChatRole, CredentialSource, TokenUsage,
        http::{BodyStream, HttpRequest, HttpResponse, HttpTransport, TransportError},
    };
    use futures_util::{StreamExt, stream};
    use reqwest::header::AUTHORIZATION;
    use std::{
        future::Future,
        sync::{Arc, Mutex},
    };
    use tokio_util::sync::CancellationToken;

    struct FakeTransport {
        status: u16,
        request_id: Option<String>,
        chunks: Vec<Vec<u8>>,
        pending_body: bool,
        request: Arc<Mutex<Option<CapturedRequest>>>,
    }

    struct CapturedRequest {
        endpoint: String,
        authorization: Option<String>,
        body: Vec<u8>,
    }

    impl FakeTransport {
        fn new(status: u16, chunks: &[&str]) -> (Self, Arc<Mutex<Option<CapturedRequest>>>) {
            let request = Arc::new(Mutex::new(None));
            (
                Self {
                    status,
                    request_id: Some("req-example-1".to_owned()),
                    chunks: chunks
                        .iter()
                        .map(|chunk| chunk.as_bytes().to_vec())
                        .collect(),
                    pending_body: false,
                    request: Arc::clone(&request),
                },
                request,
            )
        }
    }

    impl HttpTransport for FakeTransport {
        fn execute<'a>(
            &'a self,
            request: HttpRequest,
            _cancellation: &'a CancellationToken,
        ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a {
            let request_store = Arc::clone(&self.request);
            let pending_body = self.pending_body;
            let status = self.status;
            let request_id = self.request_id.clone();
            let chunks = self.chunks.clone();
            async move {
                *request_store.lock().unwrap() = Some(CapturedRequest {
                    endpoint: request.endpoint.to_string(),
                    authorization: request
                        .headers
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned),
                    body: request.body,
                });
                let body: BodyStream = if pending_body {
                    Box::pin(stream::pending::<Result<Vec<u8>, TransportError>>())
                } else {
                    Box::pin(stream::iter(
                        chunks.into_iter().map(Ok::<_, TransportError>),
                    ))
                };
                Ok(HttpResponse {
                    status,
                    request_id,
                    body,
                })
            }
        }
    }

    fn profile(provider: Provider, base_url: Option<&str>) -> AssistantProfile {
        AssistantProfile {
            provider,
            api: ApiProtocol::OpenAiChatCompletions,
            model: "example/model".to_owned(),
            base_url: base_url.map(str::to_owned),
            api_key_env: Some("EXAMPLE_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
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
        assert_eq!(request.authorization.as_deref(), Some("Bearer test-secret"));
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
}
