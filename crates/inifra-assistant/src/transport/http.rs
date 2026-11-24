use crate::{ApiProtocol, Provider};
use futures_util::{StreamExt, stream::BoxStream};
use reqwest::{Client, Method, Url, header::HeaderMap};
use std::{error::Error, fmt, future::Future, time::Duration};
use tokio_util::sync::CancellationToken;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_FAILURE: &str = "could not connect";
const TIMEOUT_FAILURE: &str = "timed out";

/// The broad cause of a provider failure, used to suggest a next step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FailureKind {
    /// The endpoint could not be reached (connection refused, DNS, TLS).
    Unreachable,
    /// The endpoint did not respond in time.
    TimedOut,
    /// The provider rejected the credential (HTTP 401 or 403).
    Unauthorized,
    /// The model or API route does not exist (HTTP 404).
    NotFound,
    /// The provider is rate limiting requests (HTTP 429).
    RateLimited,
    /// The provider failed while handling the request (HTTP 5xx).
    ServerError,
    /// The provider rejected the request for another reason (other HTTP 4xx).
    Rejected,
    /// The request failed after the response started or for a local reason.
    Other,
}

/// A provider request failure with actionable, credential-safe diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderError {
    provider: Provider,
    api: ApiProtocol,
    model: String,
    endpoint: Option<String>,
    status: Option<u16>,
    request_id: Option<String>,
    detail: String,
}

impl ProviderError {
    pub(crate) fn request(
        provider: Provider,
        api: ApiProtocol,
        model: &str,
        endpoint: &Url,
        detail: impl Into<String>,
        api_key: Option<&str>,
    ) -> Self {
        Self {
            provider,
            api,
            model: model.to_owned(),
            endpoint: Some(safe_endpoint(endpoint, api_key)),
            status: None,
            request_id: None,
            detail: detail.into(),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Response diagnostics retain the provider, endpoint, status, request ID, and credential-redaction context together."
    )]
    pub(crate) fn response(
        provider: Provider,
        api: ApiProtocol,
        model: &str,
        endpoint: &Url,
        status: u16,
        request_id: Option<String>,
        detail: impl Into<String>,
        api_key: Option<&str>,
    ) -> Self {
        let api_key = api_key.unwrap_or("");
        Self {
            provider,
            api,
            model: model.to_owned(),
            endpoint: Some(safe_endpoint(endpoint, Some(api_key))),
            status: Some(status),
            request_id: request_id.map(|value| safe_header_value(&value, api_key)),
            detail: redact_secret(&detail.into(), api_key),
        }
    }

    pub(crate) fn stream(
        provider: Provider,
        api: ApiProtocol,
        model: &str,
        endpoint: &Url,
        request_id: Option<String>,
        detail: impl Into<String>,
        api_key: Option<&str>,
    ) -> Self {
        let api_key = api_key.unwrap_or("");
        Self {
            provider,
            api,
            model: model.to_owned(),
            endpoint: Some(safe_endpoint(endpoint, Some(api_key))),
            status: None,
            request_id: request_id.map(|value| safe_header_value(&value, api_key)),
            detail: redact_secret(&detail.into(), api_key),
        }
    }

    pub(crate) fn configuration(
        provider: Provider,
        api: ApiProtocol,
        model: &str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            api,
            model: model.to_owned(),
            endpoint: None,
            status: None,
            request_id: None,
            detail: detail.into(),
        }
    }
}

impl ProviderError {
    /// Classifies the failure from its HTTP status or transport cause.
    pub fn kind(&self) -> FailureKind {
        match self.status {
            Some(401 | 403) => FailureKind::Unauthorized,
            Some(404) => FailureKind::NotFound,
            Some(429) => FailureKind::RateLimited,
            Some(500..=599) => FailureKind::ServerError,
            Some(400..=499) => FailureKind::Rejected,
            Some(_) => FailureKind::Other,
            None if self.detail.contains(CONNECT_FAILURE) => FailureKind::Unreachable,
            None if self.detail.contains(TIMEOUT_FAILURE) => FailureKind::TimedOut,
            None => FailureKind::Other,
        }
    }

    /// Provider that handled the request.
    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// Protocol used for the request.
    pub fn api(&self) -> ApiProtocol {
        self.api
    }

    /// Requested model.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Credential-safe request URL, with non-route path segments redacted.
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    /// Scheme, host, and port of the request URL.
    pub fn origin(&self) -> Option<&str> {
        let endpoint = self.endpoint.as_deref()?;
        let host_start = endpoint.find("://").map_or(0, |index| index + 3);
        let end = endpoint[host_start..]
            .find('/')
            .map_or(endpoint.len(), |index| host_start + index);
        Some(&endpoint[..end])
    }

    /// HTTP status, when the provider responded.
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// Provider request ID, when one was returned.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// Credential-safe provider or transport detail.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {} request for model {:?}",
            self.provider.display_name(),
            self.api.display_name(),
            self.model
        )?;
        if let Some(endpoint) = &self.endpoint {
            write!(formatter, " at {endpoint}")?;
        }
        formatter.write_str(" failed")?;
        if let Some(status) = self.status {
            write!(formatter, " with HTTP status {status}")?;
        }
        if let Some(request_id) = &self.request_id {
            write!(formatter, " (request ID {request_id})")?;
        }
        if !self.detail.is_empty() {
            write!(formatter, ": {}", self.detail)?;
        }
        Ok(())
    }
}

impl Error for ProviderError {}

#[cfg(test)]
mod tests {
    use super::{
        FailureKind, HttpRequest, HttpTransport, ProviderError, ReqwestTransport, TransportError,
    };
    use crate::{ApiProtocol, Provider};
    use reqwest::{Method, Url, header::HeaderMap};
    use std::{io::Read, net::TcpListener, sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;

    fn response_error(status: u16) -> ProviderError {
        ProviderError::response(
            Provider::OpenAi,
            ApiProtocol::OpenAiResponses,
            "gpt-test",
            &Url::parse("https://api.example.test/v1/responses").unwrap(),
            status,
            None,
            "provider detail",
            None,
        )
    }

    #[test]
    fn failures_are_classified_by_status_and_transport_cause() {
        assert_eq!(response_error(401).kind(), FailureKind::Unauthorized);
        assert_eq!(response_error(403).kind(), FailureKind::Unauthorized);
        assert_eq!(response_error(404).kind(), FailureKind::NotFound);
        assert_eq!(response_error(429).kind(), FailureKind::RateLimited);
        assert_eq!(response_error(503).kind(), FailureKind::ServerError);
        assert_eq!(response_error(400).kind(), FailureKind::Rejected);

        let endpoint = Url::parse("http://127.0.0.1:9/api/chat").unwrap();
        let unreachable = ProviderError::request(
            Provider::Ollama,
            ApiProtocol::OllamaChat,
            "llama",
            &endpoint,
            "could not connect: Connection refused (os error 111)",
            None,
        );
        assert_eq!(unreachable.kind(), FailureKind::Unreachable);
        assert_eq!(unreachable.origin(), Some("http://127.0.0.1:9"));
        assert_eq!(
            response_error(500).origin(),
            Some("https://api.example.test")
        );
    }

    #[tokio::test]
    async fn closed_ports_report_a_connection_failure_with_its_cause() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let transport = ReqwestTransport::new().unwrap();
        let request = HttpRequest {
            method: Method::GET,
            endpoint: Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap(),
            headers: HeaderMap::new(),
            body: None,
        };
        let Err(TransportError::Network(detail)) =
            transport.execute(request, &CancellationToken::new()).await
        else {
            panic!("a closed port must fail with a network error");
        };
        assert!(detail.starts_with("could not connect"), "{detail}");
        assert!(detail.len() > "could not connect".len(), "{detail}");
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_pending_http_send() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (accepted_sender, accepted_receiver) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            accepted_sender.send(()).unwrap();
            let mut buffer = [0; 1024];
            while connection.read(&mut buffer).unwrap_or(0) > 0 {}
        });

        let transport = ReqwestTransport::new().unwrap();
        let cancellation = CancellationToken::new();
        let request_cancellation = cancellation.clone();
        let request = HttpRequest {
            method: Method::GET,
            endpoint: Url::parse(&format!("http://{address}/pending")).unwrap(),
            headers: HeaderMap::new(),
            body: None,
        };

        let send =
            tokio::spawn(async move { transport.execute(request, &request_cancellation).await });
        tokio::task::spawn_blocking(move || accepted_receiver.recv().unwrap())
            .await
            .unwrap();
        cancellation.cancel();

        let result = tokio::time::timeout(Duration::from_secs(3), send)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(TransportError::Cancelled)));
        server.join().unwrap();
    }
}

pub(crate) type BodyStream = BoxStream<'static, Result<Vec<u8>, TransportError>>;

pub(crate) struct HttpRequest {
    pub(crate) method: Method,
    pub(crate) endpoint: Url,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Option<Vec<u8>>,
}

pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) request_id: Option<String>,
    pub(crate) body: BodyStream,
}

pub(crate) enum TransportError {
    Cancelled,
    Network(String),
}

pub(crate) trait HttpTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancellation: &'a CancellationToken,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a;
}

#[derive(Clone)]
pub(crate) struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub(crate) fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { client })
    }
}

impl HttpTransport for ReqwestTransport {
    // The explicit `Send` bound is required by the boxed response stream.
    #[allow(clippy::manual_async_fn)]
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancellation: &'a CancellationToken,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a {
        async move {
            let HttpRequest {
                method,
                endpoint,
                headers,
                body,
            } = request;
            let mut builder = self.client.request(method, endpoint).headers(headers);
            if let Some(body) = body {
                builder = builder.body(body);
            }

            let response = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(TransportError::Cancelled),
                result = builder.send() => result.map_err(network_error)?,
            };

            let request_id = ["x-request-id", "request-id"]
                .iter()
                .find_map(|name| response.headers().get(*name))
                .and_then(|value| value.to_str().ok())
                .map(|value| safe_header_value(value, ""));
            let status = response.status().as_u16();
            let body = response
                .bytes_stream()
                .map(|chunk| chunk.map(|bytes| bytes.to_vec()).map_err(network_error))
                .boxed();
            Ok(HttpResponse {
                status,
                request_id,
                body,
            })
        }
    }
}

/// Describes a reqwest failure with a stable cause prefix and the innermost source message,
/// which is where connection errors such as "Connection refused" are reported.
fn network_error(error: reqwest::Error) -> TransportError {
    let cause = if error.is_connect() {
        Some(CONNECT_FAILURE)
    } else if error.is_timeout() {
        Some(TIMEOUT_FAILURE)
    } else {
        None
    };
    let error = error.without_url();
    let mut root: &dyn Error = &error;
    while let Some(source) = root.source() {
        root = source;
    }
    let root = root.to_string();
    let summary = error.to_string();
    let detail = match cause {
        Some(cause) if root != summary => format!("{cause}: {root}"),
        Some(cause) => cause.to_owned(),
        None if root != summary => format!("{summary}: {root}"),
        None => summary,
    };
    TransportError::Network(detail)
}

pub(crate) fn safe_excerpt(bytes: &[u8], api_key: Option<&str>) -> String {
    let body = String::from_utf8_lossy(bytes);
    let body = if let Some(api_key) = api_key.filter(|key| !key.is_empty()) {
        body.replace(api_key, "[redacted]")
    } else {
        body.into_owned()
    };
    body.chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\t') {
                ' '
            } else {
                character
            }
        })
        .take(512)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn safe_header_value(value: &str, api_key: &str) -> String {
    let value = value
        .chars()
        .filter(|character| !character.is_control())
        .take(128)
        .collect::<String>();
    redact_secret(&value, api_key)
}

fn safe_endpoint(endpoint: &Url, api_key: Option<&str>) -> String {
    let origin = endpoint.origin().ascii_serialization();
    let endpoint = if endpoint.path() == "/" {
        origin
    } else {
        let path = endpoint.path();
        let route = if path.contains("/models/") && path.ends_with(":streamGenerateContent") {
            Some("/models/[model]:streamGenerateContent")
        } else {
            [
                "/chat/completions",
                "/api/chat",
                "/responses",
                "/messages",
                "/generateContent",
            ]
            .into_iter()
            .find(|suffix| path.ends_with(suffix))
        };
        match route {
            Some(route) => format!("{origin}/[path redacted]{route}"),
            None => format!("{origin}/[path redacted]"),
        }
    };
    api_key.map_or(endpoint.clone(), |key| redact_secret(&endpoint, key))
}

fn redact_secret(value: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        value.to_owned()
    } else {
        value.replace(api_key, "[redacted]")
    }
}
