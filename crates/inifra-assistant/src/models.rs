use crate::{
    ApiProtocol, AssistantProfile, Provider, ProviderError,
    adapter::{add_header, endpoint_url},
    http::{HttpRequest, HttpTransport, ReqwestTransport, TransportError, safe_excerpt},
};
use futures_util::StreamExt;
use reqwest::{Method, Url, header};
use serde_json::Value;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const ANTHROPIC_API_VERSION: &str = "2026-10-21";

/// Lists model identifiers offered by the configured provider API, when supported.
///
/// Compatible servers may not expose a model-list endpoint. Responses are size-limited,
/// redirects are disabled so credentials are not forwarded to another origin, and diagnostics
/// redact API keys.
///
/// # Errors
/// Returns a credential-safe error when the endpoint cannot be reached or its response is invalid.
pub async fn list_models(
    profile: &AssistantProfile,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProviderError> {
    let transport = ReqwestTransport::new().map_err(|error| {
        ProviderError::configuration(
            profile.provider,
            profile.api,
            &profile.model,
            format!("could not initialize HTTPS client: {}", error.without_url()),
        )
    })?;
    list_models_with_transport(&transport, profile, api_key, &CancellationToken::new()).await
}

async fn list_models_with_transport<T: HttpTransport>(
    transport: &T,
    profile: &AssistantProfile,
    api_key: Option<&str>,
    cancellation: &CancellationToken,
) -> Result<Vec<String>, ProviderError> {
    let endpoint = models_endpoint(profile).map_err(|detail| {
        ProviderError::configuration(profile.provider, profile.api, &profile.model, detail)
    })?;
    profile.validate().map_err(|error| {
        ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            error.to_string(),
            api_key,
        )
    })?;
    let api_key = api_key.filter(|key| !key.trim().is_empty());
    if profile.provider != Provider::Ollama && api_key.is_none() {
        return Err(ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            "API key is missing; configure the profile credential",
            None,
        ));
    }

    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );
    if let Some(api_key) = api_key {
        let (name, value) = match profile.api {
            ApiProtocol::AnthropicMessages => (
                header::HeaderName::from_static("x-api-key"),
                api_key.to_owned(),
            ),
            ApiProtocol::GeminiGenerateContent => (
                header::HeaderName::from_static("x-goog-api-key"),
                api_key.to_owned(),
            ),
            ApiProtocol::OllamaChat => (header::AUTHORIZATION, format!("Bearer {api_key}")),
            ApiProtocol::OpenAiResponses | ApiProtocol::OpenAiChatCompletions => {
                (header::AUTHORIZATION, format!("Bearer {api_key}"))
            }
        };
        add_header(&mut headers, name, &value).map_err(|detail| {
            ProviderError::request(
                profile.provider,
                profile.api,
                &profile.model,
                &endpoint,
                detail,
                Some(api_key),
            )
        })?;
    }
    if profile.api == ApiProtocol::AnthropicMessages {
        add_header(
            &mut headers,
            header::HeaderName::from_static("anthropic-version"),
            ANTHROPIC_API_VERSION,
        )
        .map_err(|detail| {
            ProviderError::request(
                profile.provider,
                profile.api,
                &profile.model,
                &endpoint,
                detail,
                api_key,
            )
        })?;
    }

    let request = HttpRequest {
        method: Method::GET,
        endpoint: endpoint.clone(),
        headers,
        body: None,
    };
    let response = tokio::time::timeout(REQUEST_TIMEOUT, async {
        let response = transport.execute(request, cancellation).await?;
        let status = response.status;
        let request_id = response.request_id;
        let body = read_bounded(response.body, cancellation).await;
        Ok::<_, TransportError>((status, request_id, body))
    })
    .await
    .map_err(|_| {
        ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            format!(
                "model-list request timed out after {} seconds",
                REQUEST_TIMEOUT.as_secs()
            ),
            api_key,
        )
    })?
    .map_err(|error| match error {
        TransportError::Cancelled => ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            "model-list request cancelled",
            api_key,
        ),
        TransportError::Network(detail) => ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            format!(
                "model-list request failed: {}",
                safe_excerpt(detail.as_bytes(), api_key)
            ),
            api_key,
        ),
    })?;
    let (status, request_id, body) = response;
    let body = body.map_err(|error| {
        let detail = match error {
            TransportError::Cancelled => "model-list request cancelled".to_owned(),
            TransportError::Network(detail) => {
                format!("could not read model-list response: {detail}")
            }
        };
        ProviderError::response(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            status,
            request_id.clone(),
            safe_excerpt(detail.as_bytes(), api_key),
            api_key,
        )
    })?;
    if !(200..300).contains(&status) {
        let detail = safe_excerpt(&body, api_key)
            .chars()
            .take(400)
            .collect::<String>();
        return Err(ProviderError::response(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            status,
            request_id,
            if detail.is_empty() {
                "provider returned an empty error response".to_owned()
            } else {
                format!("provider returned: {detail}")
            },
            api_key,
        ));
    }
    decode_models(profile.api, &body).map_err(|detail| {
        ProviderError::response(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            status,
            request_id,
            detail,
            api_key,
        )
    })
}

fn models_endpoint(profile: &AssistantProfile) -> Result<Url, String> {
    let (base, path) = match (profile.provider, profile.api) {
        (Provider::OpenAi, ApiProtocol::OpenAiResponses | ApiProtocol::OpenAiChatCompletions) => (
            profile
                .base_url
                .as_deref()
                .unwrap_or("https://api.openai.com/v1"),
            "/models",
        ),
        (Provider::NvidiaNim, ApiProtocol::OpenAiChatCompletions) => (
            profile
                .base_url
                .as_deref()
                .unwrap_or("https://integrate.api.nvidia.com/v1"),
            "/models",
        ),
        (Provider::OpenRouter, ApiProtocol::OpenAiChatCompletions) => (
            profile
                .base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api/v1"),
            "/models",
        ),
        (Provider::OpenAiCompatible, ApiProtocol::OpenAiChatCompletions) => (
            profile
                .base_url
                .as_deref()
                .ok_or_else(|| "custom endpoint requires an API base URL".to_owned())?,
            "/models",
        ),
        (Provider::Anthropic, ApiProtocol::AnthropicMessages) => (
            profile
                .base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com/v1"),
            "/models",
        ),
        (Provider::Gemini, ApiProtocol::GeminiGenerateContent) => (
            profile
                .base_url
                .as_deref()
                .unwrap_or("https://generativelanguage.googleapis.com/v1beta"),
            "/models",
        ),
        (Provider::Ollama, ApiProtocol::OllamaChat) => (
            profile
                .base_url
                .as_deref()
                .unwrap_or("http://localhost:11434"),
            "/api/tags",
        ),
        _ => return Err("profile provider and API protocol do not match".to_owned()),
    };
    let base = if profile.api == ApiProtocol::OpenAiChatCompletions {
        without_chat_completion_route(base)?
    } else {
        base.to_owned()
    };
    endpoint_url(&base, path)
}

fn without_chat_completion_route(base: &str) -> Result<String, String> {
    let mut url = Url::parse(base).map_err(|_| "API base URL is invalid".to_owned())?;
    let path = url.path().trim_end_matches('/').to_owned();
    if let Some(base_path) = path.strip_suffix("/chat/completions") {
        url.set_path(base_path);
    }
    Ok(url.to_string())
}

async fn read_bounded(
    mut stream: crate::http::BodyStream,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, TransportError> {
    let mut body = Vec::new();
    loop {
        let next = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(TransportError::Cancelled),
            next = stream.next() => next,
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk?;
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(TransportError::Network(format!(
                "model-list response exceeded the {} KiB limit",
                MAX_RESPONSE_BYTES / 1024
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn decode_models(api: ApiProtocol, body: &[u8]) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|_| {
        format!(
            "provider returned invalid model-list JSON: {}",
            safe_excerpt(body, None)
        )
    })?;
    let models = match api {
        ApiProtocol::GeminiGenerateContent => value
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|model| {
                model
                    .get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .is_some_and(|methods| {
                        methods
                            .iter()
                            .any(|method| method.as_str() == Some("generateContent"))
                    })
            })
            .filter_map(|model| model.get("name").and_then(Value::as_str))
            .map(|name| name.strip_prefix("models/").unwrap_or(name).to_owned())
            .collect::<Vec<_>>(),
        ApiProtocol::OllamaChat => value
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| {
                model
                    .get("model")
                    .or_else(|| model.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>(),
        ApiProtocol::OpenAiResponses
        | ApiProtocol::OpenAiChatCompletions
        | ApiProtocol::AnthropicMessages => value
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>(),
    };
    let mut models = models
        .into_iter()
        .filter(|model| !model.is_empty() && !model.chars().any(char::is_control))
        .collect::<Vec<_>>();
    models.sort_unstable();
    models.dedup();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::{decode_models, list_models_with_transport, models_endpoint};
    use crate::{
        ApiProtocol, AssistantProfile, CredentialSource, Provider,
        test_support::{CapturedRequest, FakeTransport},
    };
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    fn profile(provider: Provider, api: ApiProtocol, base_url: Option<&str>) -> AssistantProfile {
        AssistantProfile {
            provider,
            api,
            model: "test-model".to_owned(),
            base_url: base_url.map(str::to_owned),
            api_key_env: (provider != Provider::Ollama).then(|| "TEST_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
        }
    }

    #[test]
    fn decodes_and_sorts_openai_compatible_models() {
        let models = decode_models(
            ApiProtocol::OpenAiChatCompletions,
            br#"{"data":[{"id":"zeta"},{"id":"alpha"},{"id":"alpha"},{"id":""}]}"#,
        )
        .unwrap();
        assert_eq!(models, ["alpha", "zeta"]);
    }

    #[test]
    fn gemini_models_include_only_generation_capable_ids() {
        let models = decode_models(
            ApiProtocol::GeminiGenerateContent,
            br#"{"models":[{"name":"models/gemini-a","supportedGenerationMethods":["generateContent"]},{"name":"models/embed-a","supportedGenerationMethods":["embedContent"]}]}"#,
        )
        .unwrap();
        assert_eq!(models, ["gemini-a"]);
    }

    #[test]
    fn ollama_models_accept_model_and_legacy_name_fields() {
        let models = decode_models(
            ApiProtocol::OllamaChat,
            br#"{"models":[{"model":"llama3.2:latest"},{"name":"qwen3"}]}"#,
        )
        .unwrap();
        assert_eq!(models, ["llama3.2:latest", "qwen3"]);
    }

    #[test]
    fn model_endpoints_preserve_api_prefixes_and_use_provider_defaults() {
        let custom = models_endpoint(&profile(
            Provider::OpenAiCompatible,
            ApiProtocol::OpenAiChatCompletions,
            Some("https://api.example.test/gateway/v1/"),
        ))
        .unwrap();
        assert_eq!(
            custom.as_str(),
            "https://api.example.test/gateway/v1/models"
        );

        let full_route = models_endpoint(&profile(
            Provider::OpenAiCompatible,
            ApiProtocol::OpenAiChatCompletions,
            Some("https://api.example.test/gateway/v1/chat/completions/"),
        ))
        .unwrap();
        assert_eq!(
            full_route.as_str(),
            "https://api.example.test/gateway/v1/models"
        );

        let ollama =
            models_endpoint(&profile(Provider::Ollama, ApiProtocol::OllamaChat, None)).unwrap();
        assert_eq!(ollama.as_str(), "http://localhost:11434/api/tags");
    }

    #[test]
    fn rejects_mismatched_provider_protocols() {
        let error = models_endpoint(&profile(
            Provider::Anthropic,
            ApiProtocol::OpenAiResponses,
            None,
        ))
        .unwrap_err();
        assert!(error.contains("do not match"));
    }

    #[tokio::test]
    async fn model_listing_uses_get_and_the_configured_credential() {
        let (transport, captured) = FakeTransport::new(200, &[r#"{"data":[{"id":"model-a"}]}"#]);
        let profile = profile(
            Provider::OpenAiCompatible,
            ApiProtocol::OpenAiChatCompletions,
            Some("https://api.example.test/v1"),
        );

        let models = list_models_with_transport(
            &transport,
            &profile,
            Some("test-secret"),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(models, ["model-a"]);
        let request: CapturedRequest = captured.lock().unwrap().take().unwrap();
        assert_eq!(request.method, reqwest::Method::GET);
        assert_eq!(request.endpoint, "https://api.example.test/v1/models");
        assert_eq!(request.header("authorization"), Some("Bearer test-secret"));
        assert!(request.body.is_empty());
    }

    #[tokio::test]
    async fn model_listing_errors_redact_the_configured_credential() {
        let (transport, _) = FakeTransport::new(401, &[r#"{"error":"bad test-secret"}"#]);
        let profile = profile(
            Provider::OpenAiCompatible,
            ApiProtocol::OpenAiChatCompletions,
            Some("https://api.example.test/v1"),
        );

        let error = list_models_with_transport(
            &transport,
            &profile,
            Some("test-secret"),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(!error.contains("test-secret"));
        assert!(error.contains("[redacted]"));
    }

    #[tokio::test]
    async fn model_listing_can_be_cancelled_while_reading_the_response() {
        let (mut transport, _) = FakeTransport::new(200, &[]);
        transport.pending_body = true;
        let profile = profile(
            Provider::OpenAiCompatible,
            ApiProtocol::OpenAiChatCompletions,
            Some("https://api.example.test/v1"),
        );
        let cancellation = CancellationToken::new();
        let cancel_request = cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel_request.cancel();
        });

        let error =
            list_models_with_transport(&transport, &profile, Some("test-secret"), &cancellation)
                .await
                .unwrap_err()
                .to_string();

        assert!(error.contains("cancelled"));
    }
}
