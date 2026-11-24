use crate::{
    AssistantProfile, ChatMessage, ChatStream, ProviderError,
    transport::http::{HttpRequest, HttpTransport},
    transport::stream::{FrameDecoder, FrameParser, stream_with_transport},
};
use reqwest::{Url, header};
use tokio_util::sync::CancellationToken;

pub(crate) type RequestBuilder =
    fn(&AssistantProfile, Vec<ChatMessage>, Option<&str>, Url) -> Result<HttpRequest, String>;

pub(crate) struct AdapterSpec<P> {
    pub(crate) endpoint: Result<Url, String>,
    pub(crate) build_request: RequestBuilder,
    pub(crate) parser: P,
    pub(crate) decode: FrameDecoder,
}

pub(crate) fn start_stream<T, P>(
    transport: T,
    profile: AssistantProfile,
    api_key: Option<String>,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
    spec: AdapterSpec<P>,
) -> ChatStream
where
    T: HttpTransport + 'static,
    P: FrameParser + 'static,
{
    let endpoint = match spec.endpoint {
        Ok(endpoint) => endpoint,
        Err(message) => {
            return one_error(ProviderError::configuration(
                profile.provider,
                profile.api,
                &profile.model,
                message,
            ));
        }
    };
    if let Err(error) = profile.validate() {
        return one_error(ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            error.to_string(),
            None,
        ));
    }
    if messages.is_empty() {
        return one_error(ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            "chat request requires at least one message",
            None,
        ));
    }

    let api_key = api_key.filter(|key| !key.trim().is_empty());
    if profile.api_key_required() && api_key.is_none() {
        return one_error(ProviderError::request(
            profile.provider,
            profile.api,
            &profile.model,
            &endpoint,
            "API key is missing; configure the profile credential",
            None,
        ));
    }

    let request =
        match (spec.build_request)(&profile, messages, api_key.as_deref(), endpoint.clone()) {
            Ok(request) => request,
            Err(message) => {
                return one_error(ProviderError::request(
                    profile.provider,
                    profile.api,
                    &profile.model,
                    &endpoint,
                    message,
                    api_key.as_deref(),
                ));
            }
        };
    stream_with_transport(
        transport,
        profile,
        api_key,
        request,
        cancellation,
        spec.parser,
        spec.decode,
    )
}

pub(crate) fn one_error(error: ProviderError) -> ChatStream {
    Box::pin(futures_util::stream::once(async move { Err(error) }))
}

pub(crate) fn json_headers(accept: &'static str) -> header::HeaderMap {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, header::HeaderValue::from_static(accept));
    headers
}

pub(crate) fn add_header(
    headers: &mut header::HeaderMap,
    name: header::HeaderName,
    value: &str,
) -> Result<(), String> {
    let value = header::HeaderValue::from_str(value)
        .map_err(|_| "credential cannot be represented as an HTTP header".to_owned())?;
    headers.insert(name, value);
    Ok(())
}

pub(crate) fn bearer_headers(
    api_key: Option<&str>,
    accept: &'static str,
) -> Result<header::HeaderMap, String> {
    let mut headers = json_headers(accept);
    if let Some(api_key) = api_key {
        add_header(
            &mut headers,
            header::AUTHORIZATION,
            &format!("Bearer {api_key}"),
        )?;
    }
    Ok(headers)
}

pub(crate) fn endpoint_url(base: &str, suffix: &str) -> Result<Url, String> {
    let mut url = Url::parse(base).map_err(|_| "API base URL is invalid".to_owned())?;
    let path = url.path().trim_end_matches('/');
    let suffix = format!("/{}", suffix.trim_start_matches('/'));
    let path = if path.ends_with(&suffix) {
        path.to_owned()
    } else {
        format!("{path}{suffix}")
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}
