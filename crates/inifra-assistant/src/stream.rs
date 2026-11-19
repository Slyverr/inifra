use crate::{
    AssistantProfile, ChatEvent, ChatStream, ProviderError,
    http::{BodyStream, HttpRequest, HttpTransport, TransportError, safe_excerpt},
};
use async_stream::stream;
use futures_util::StreamExt;
use reqwest::Url;
use tokio_util::sync::CancellationToken;

const MAX_ERROR_BODY_BYTES: usize = 16 * 1024;

pub(crate) struct DecodedFrame {
    pub(crate) events: Vec<ChatEvent>,
    pub(crate) done: bool,
}

impl DecodedFrame {
    pub(crate) fn events(events: Vec<ChatEvent>) -> Self {
        Self {
            events,
            done: false,
        }
    }

    pub(crate) fn done(events: Vec<ChatEvent>) -> Self {
        Self { events, done: true }
    }
}

pub(crate) trait FrameParser: Send {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, &'static str>;
    fn finish(&mut self) -> Result<Vec<String>, &'static str>;
}

pub(crate) type FrameDecoder = for<'a> fn(
    &str,
    &AssistantProfile,
    &Url,
    Option<&str>,
    Option<&str>,
) -> Result<DecodedFrame, ProviderError>;

pub(crate) fn stream_with_transport<T, P>(
    transport: T,
    profile: AssistantProfile,
    api_key: Option<String>,
    request: HttpRequest,
    cancellation: CancellationToken,
    mut parser: P,
    decode: FrameDecoder,
) -> ChatStream
where
    T: HttpTransport + 'static,
    P: FrameParser + 'static,
{
    Box::pin(stream! {
        let endpoint = request.endpoint.clone();
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
                    safe_excerpt(message.as_bytes(), api_key.as_deref()),
                    api_key.as_deref(),
                ));
                return;
            }
        };
        let status = response.status;
        let request_id = response.request_id;
        let mut body = response.body;
        if !(200..300).contains(&status) {
            let (body, read_error) = match read_error_body(body, &cancellation).await {
                Ok(body) => body,
                Err(TransportError::Cancelled) => {
                    yield Ok(ChatEvent::Cancelled);
                    return;
                }
                Err(TransportError::Network(message)) => (Vec::new(), Some(message)),
            };
            let mut detail = error_message(&body, api_key.as_deref());
            if let Some(read_error) = read_error {
                if !detail.is_empty() {
                    detail.push_str("; ");
                }
                detail.push_str("could not read the error response: ");
                detail.push_str(&safe_excerpt(read_error.as_bytes(), api_key.as_deref()));
            }
            yield Err(ProviderError::response(
                profile.provider,
                profile.api,
                &profile.model,
                &endpoint,
                status,
                request_id,
                detail,
                api_key.as_deref(),
            ));
            return;
        }

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
                        request_id.clone(),
                        safe_excerpt(message.as_bytes(), api_key.as_deref()),
                        api_key.as_deref(),
                    ));
                    return;
                }
            };
            let frames = match parser.push(&chunk) {
                Ok(frames) => frames,
                Err(message) => {
                    yield Err(ProviderError::stream(
                        profile.provider,
                        profile.api,
                        &profile.model,
                        &endpoint,
                        request_id.clone(),
                        message,
                        api_key.as_deref(),
                    ));
                    return;
                }
            };
            for frame in frames {
                match decode(
                    &frame,
                    &profile,
                    &endpoint,
                    request_id.as_deref(),
                    api_key.as_deref(),
                ) {
                    Ok(decoded) => {
                        let done = decoded.done;
                        for event in decoded.events {
                            yield Ok(event);
                        }
                        if done {
                            return;
                        }
                    }
                    Err(error) => {
                        yield Err(error);
                        return;
                    }
                }
            }
        }

        let frames = match parser.finish() {
            Ok(frames) => frames,
            Err(message) => {
                yield Err(ProviderError::stream(
                    profile.provider,
                    profile.api,
                    &profile.model,
                    &endpoint,
                    request_id.clone(),
                    message,
                    api_key.as_deref(),
                ));
                return;
            }
        };
        for frame in frames {
            match decode(
                &frame,
                &profile,
                &endpoint,
                request_id.as_deref(),
                api_key.as_deref(),
            ) {
                Ok(decoded) => {
                    let done = decoded.done;
                    for event in decoded.events {
                        yield Ok(event);
                    }
                    if done {
                        return;
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

fn error_message(bytes: &[u8], api_key: Option<&str>) -> String {
    let message = serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    message.map_or_else(
        || safe_excerpt(bytes, api_key),
        |message| safe_excerpt(message.as_bytes(), api_key),
    )
}
