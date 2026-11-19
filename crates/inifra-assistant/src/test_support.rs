use crate::http::{BodyStream, HttpRequest, HttpResponse, HttpTransport, TransportError};
use futures_util::stream;
use reqwest::{Method, header::HeaderMap};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

pub(crate) struct CapturedRequest {
    pub(crate) method: Method,
    pub(crate) endpoint: String,
    headers: BTreeMap<String, String>,
    pub(crate) body: Vec<u8>,
}

impl CapturedRequest {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

pub(crate) struct FakeTransport {
    pub(crate) status: u16,
    pub(crate) request_id: Option<String>,
    chunks: Vec<Vec<u8>>,
    pub(crate) pending_body: bool,
    request: Arc<Mutex<Option<CapturedRequest>>>,
}

impl FakeTransport {
    pub(crate) fn new(status: u16, chunks: &[&str]) -> (Self, Arc<Mutex<Option<CapturedRequest>>>) {
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
                method: request.method,
                endpoint: request.endpoint.to_string(),
                headers: capture_headers(&request.headers),
                body: request.body.unwrap_or_default(),
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

fn capture_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect()
}
