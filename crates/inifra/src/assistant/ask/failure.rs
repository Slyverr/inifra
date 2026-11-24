//! Turns provider and credential failures into a summary, a next step, and muted details.
//!
//! Messages follow the chat error convention: the first line is the summary, later lines are
//! guidance with `commands` in backticks, and a final `details: ` line keeps the raw cause.

use inifra_assistant::{
    ApiProtocol, AssistantProfile, CredentialSource, FailureKind, Provider, ProviderError,
};

/// Why a background request did not produce a result.
#[derive(Debug)]
pub(super) enum RequestFailure {
    /// The user cancelled the request.
    Cancelled,
    /// The provider or transport reported an error.
    Provider(ProviderError),
    /// A local limit or setup step failed; the text is already user-facing.
    Other(String),
}

impl RequestFailure {
    pub(super) fn describe(&self, profile_id: &str, profile: &AssistantProfile) -> String {
        match self {
            Self::Cancelled => "Request cancelled.".to_owned(),
            Self::Provider(error) => describe_provider_error(error, profile_id, profile),
            Self::Other(message) => message.clone(),
        }
    }
}

/// Explains a provider failure and suggests the most likely fix.
pub(super) fn describe_provider_error(
    error: &ProviderError,
    profile_id: &str,
    profile: &AssistantProfile,
) -> String {
    let provider = error.provider().display_name();
    let configure = format!("`inifra assistant configure {profile_id}`");
    let status = error
        .status()
        .map(|status| format!(" (HTTP {status})"))
        .unwrap_or_default();

    let (summary, hint) = match error.kind() {
        FailureKind::Unreachable => (
            match error.origin() {
                Some(origin) => format!("Could not reach {provider} at {origin}."),
                None => format!("Could not reach {provider}."),
            },
            if error.provider() == Provider::Ollama {
                format!("Is the Ollama server running? Check the base URL with {configure}.")
            } else {
                format!("Check your network connection, or the base URL with {configure}.")
            },
        ),
        FailureKind::TimedOut => (
            format!("{provider} did not respond in time."),
            format!("Try again, or check the base URL with {configure}."),
        ),
        FailureKind::Unauthorized => (
            format!("{provider} rejected the API key{status}."),
            match (&profile.api_key_env, profile.credential_source) {
                (Some(name), CredentialSource::Environment) => {
                    format!("Check `{name}`, or save a key with {configure}.")
                }
                _ => format!("Save a new key with {configure}."),
            },
        ),
        FailureKind::NotFound => (
            format!(
                "{provider} could not find model {} or the API route{status}.",
                error.model()
            ),
            if error.api() == ApiProtocol::OpenAiChatCompletions && profile.base_url.is_some() {
                format!(
                    "The base URL should be the API root, such as https://host/v1; Inifra adds /chat/completions. List models with `/models`, or fix the URL with {configure}."
                )
            } else {
                format!("List models with `/models`, or check the base URL with {configure}.")
            },
        ),
        FailureKind::RateLimited => (
            format!("{provider} is rate limiting requests{status}."),
            "Wait a moment and try again, or switch with `/model` or `/profile`.".to_owned(),
        ),
        FailureKind::ServerError => (
            format!("{provider} had a server error{status}."),
            "Try again shortly.".to_owned(),
        ),
        FailureKind::Rejected => (
            format!("{provider} rejected the request{status}."),
            "The details below come from the provider. `/model ID` switches models for this session."
                .to_owned(),
        ),
        _ => (format!("The {provider} request failed{status}."), String::new()),
    };

    let mut details = error.detail().to_owned();
    if let Some(request_id) = error.request_id() {
        details.push_str(&format!(" · request ID {request_id}"));
    }

    let mut lines = vec![summary];
    if !hint.is_empty() {
        lines.push(hint);
    }
    if !details.trim().is_empty() {
        lines.push(format!("details: {}", single_line(&details)));
    }
    lines.join("\n")
}

/// Explains why `/profile` could not switch to a profile whose credential is unavailable.
pub(super) fn describe_switch_failure(
    target_id: &str,
    target: &AssistantProfile,
    current_id: &str,
    detail: &str,
) -> String {
    let configure = format!("`inifra assistant configure {target_id}`");
    let (summary, hint) = match (&target.api_key_env, target.credential_source) {
        (Some(name), CredentialSource::Environment) => (
            format!("Can't switch to {target_id}: {name} is not set."),
            format!("Export it, or save a key with {configure}."),
        ),
        (Some(name), CredentialSource::CredentialsFile) => (
            format!("Can't switch to {target_id}: no saved key for {name}."),
            format!("Save one with {configure}."),
        ),
        (None, _) => (
            format!("Can't switch to {target_id}: its credential could not be loaded."),
            format!("Check it with {configure}."),
        ),
    };
    [
        summary,
        hint,
        format!("Still using {current_id}."),
        format!("details: {}", single_line(detail)),
    ]
    .join("\n")
}

fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{describe_provider_error, describe_switch_failure};
    use inifra_assistant::{
        ApiProtocol, AssistantProfile, CancellationToken, ChatMessage, ChatRole, CredentialSource,
        Provider, ProviderClient,
    };

    fn profile(provider: Provider, api: ApiProtocol) -> AssistantProfile {
        AssistantProfile {
            provider,
            api,
            model: "test-model".to_owned(),
            base_url: None,
            api_key_env: Some("TEST_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
        }
    }

    fn closed_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    /// Sends one real request to a closed local port and returns the provider error.
    fn unreachable_error(profile: &AssistantProfile) -> inifra_assistant::ProviderError {
        use futures_util::StreamExt;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let client = ProviderClient::new().unwrap();
            let mut stream = client.stream_chat(
                profile.clone(),
                None,
                vec![ChatMessage::new(ChatRole::User, "hi")],
                CancellationToken::new(),
            );
            loop {
                match stream.next().await {
                    Some(Err(error)) => return error,
                    Some(Ok(_)) => continue,
                    None => panic!("request to a closed port did not fail"),
                }
            }
        })
    }

    #[test]
    fn unreachable_ollama_suggests_checking_the_server_and_names_the_origin() {
        let port = closed_port();
        let mut profile = profile(Provider::Ollama, ApiProtocol::OllamaChat);
        profile.api_key_env = None;
        profile.base_url = Some(format!("http://127.0.0.1:{port}"));
        let message = describe_provider_error(&unreachable_error(&profile), "local", &profile);
        let lines = message.lines().collect::<Vec<_>>();

        assert_eq!(
            lines[0],
            format!("Could not reach Ollama at http://127.0.0.1:{port}.")
        );
        assert!(lines[1].starts_with("Is the Ollama server running?"));
        assert!(lines[1].contains("`inifra assistant configure local`"));
        assert!(lines[2].starts_with("details: could not connect"));
    }

    #[test]
    fn switch_failures_name_the_missing_variable_and_keep_the_current_profile() {
        let target = profile(Provider::OpenAi, ApiProtocol::OpenAiResponses);
        let message = describe_switch_failure("work", &target, "local", "missing\ncredential");
        let lines = message.lines().collect::<Vec<_>>();
        assert_eq!(lines[0], "Can't switch to work: TEST_API_KEY is not set.");
        assert!(lines[1].contains("`inifra assistant configure work`"));
        assert_eq!(lines[2], "Still using local.");
        assert_eq!(lines[3], "details: missing credential");
    }
}
