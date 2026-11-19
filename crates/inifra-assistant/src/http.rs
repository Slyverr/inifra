use crate::{ApiProtocol, Provider};
use futures_util::{StreamExt, stream::BoxStream};
use reqwest::{Client, Url, header::HeaderMap};
use std::{error::Error, fmt, future::Future, time::Duration};
use tokio_util::sync::CancellationToken;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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
        api_key: &str,
    ) -> Self {
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
        api_key: &str,
    ) -> Self {
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
    use super::{HttpRequest, HttpTransport, ReqwestTransport, TransportError};
    use reqwest::{Url, header::HeaderMap};
    use std::{io::Read, net::TcpListener, sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;

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
            endpoint: Url::parse(&format!("http://{address}/pending")).unwrap(),
            headers: HeaderMap::new(),
            body: Vec::new(),
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
    pub(crate) endpoint: Url,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
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
            let response = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(TransportError::Cancelled),
                result = self.client.post(request.endpoint).headers(request.headers).body(request.body).send() => {
                result.map_err(|error| TransportError::Network(error.without_url().to_string()))?
                }
            };
            let request_id = ["x-request-id", "request-id"]
                .iter()
                .find_map(|name| response.headers().get(*name))
                .and_then(|value| value.to_str().ok())
                .map(|value| safe_header_value(value, ""));
            let status = response.status().as_u16();
            let body = response
                .bytes_stream()
                .map(|chunk| {
                    chunk
                        .map(|bytes| bytes.to_vec())
                        .map_err(|error| TransportError::Network(error.without_url().to_string()))
                })
                .boxed();
            Ok(HttpResponse {
                status,
                request_id,
                body,
            })
        }
    }
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
        let route = [
            "/chat/completions",
            "/responses",
            "/messages",
            "/generateContent",
        ]
        .into_iter()
        .find(|suffix| endpoint.path().ends_with(suffix));
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
