use super::{
    AssistantProfile, MAX_HISTORY_BYTES, MAX_HISTORY_MESSAGES, MAX_RESPONSE_BYTES,
    MAX_USER_MESSAGE_BYTES, Session, apply_confirmation, commands, failure::RequestFailure,
    handle_command, history_size, local_context_message, switching,
};
use futures_util::StreamExt;
use inifra_assistant::{
    CancellationToken, ChatEvent as ProviderEvent, ChatMessage, ChatRole, ProviderClient,
    TokenUsage, list_models,
};
use inifra_tui::{ChatAction, ChatEvent, ChatUsage};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

const MAX_MODEL_SUGGESTIONS: usize = 100;

enum ActiveKind {
    Chat { user_message: String },
    Models,
}

pub(super) struct ActiveRequest {
    cancellation: CancellationToken,
    kind: ActiveKind,
    thread: JoinHandle<()>,
}

/// Why a background model listing failed.
pub(super) enum RuntimeError {
    /// The async runtime could not start.
    Runtime(String),
    /// The provider request failed.
    Provider(inifra_assistant::ProviderError),
}

impl From<std::io::Error> for RuntimeError {
    fn from(error: std::io::Error) -> Self {
        Self::Runtime(format!("Could not start the model-list runtime: {error}"))
    }
}

pub(super) enum WorkerResult {
    /// Model IDs for a profile shown in the switcher; not tied to the active request.
    SwitcherModels {
        profile: String,
        result: Result<Vec<String>, RuntimeError>,
    },
    Chat {
        response: Result<String, RequestFailure>,
        usage: TokenUsage,
    },
    Models(Result<Vec<String>, RequestFailure>),
}

pub(super) fn run_session(
    mut session: Session,
    actions: Receiver<ChatAction>,
    events: Sender<ChatEvent>,
) {
    let (worker_tx, worker_rx) = mpsc::channel();
    let mut active: Option<ActiveRequest> = None;
    session.send_session(&events);
    session.send_completions("/", &events);

    loop {
        while let Ok(result) = worker_rx.try_recv() {
            match result {
                WorkerResult::SwitcherModels { profile, result } => {
                    switching::finish_models(profile, result, &mut session, &events);
                }
                result => finish_worker(result, &mut session, &events, &mut active),
            }
        }

        match actions.recv_timeout(Duration::from_millis(40)) {
            Ok(ChatAction::Quit) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(active) = active.take() {
                    active.cancellation.cancel();
                    let _ = active.thread.join();
                }
                break;
            }
            Ok(ChatAction::Cancel) => {
                if let Some(active) = &active {
                    active.cancellation.cancel();
                }
            }
            Ok(ChatAction::Complete(input)) => {
                if commands::requests_model_completion(&input, &session.known_models) {
                    start_models(&session, &events, &worker_tx, &mut active);
                } else {
                    session.send_completions(&input, &events);
                }
            }

            Ok(ChatAction::Confirm {
                request_id,
                approved,
            }) => apply_confirmation(request_id, approved, &mut session, &events),
            Ok(ChatAction::OpenSwitcher) => {
                if active.is_some() || session.pending_confirmation.is_some() {
                    let _ = events.send(ChatEvent::Error(
                        "Wait for the current operation to finish.".to_owned(),
                    ));
                } else {
                    switching::open_switcher(&session, &events);
                }
            }
            Ok(ChatAction::LoadModels(profile)) => {
                switching::load_models(profile, &session, &events, &worker_tx);
            }
            Ok(ChatAction::Switch(request)) => {
                if active.is_some() || session.pending_confirmation.is_some() {
                    let _ = events.send(ChatEvent::Error(
                        "Wait for the current operation to finish.".to_owned(),
                    ));
                } else {
                    switching::apply_switch(request, &mut session, &events);
                }
            }
            Ok(ChatAction::Submit(input)) => {
                if active.is_some() || session.pending_confirmation.is_some() {
                    let _ = events.send(ChatEvent::Error(
                        "Wait for the current operation to finish.".to_owned(),
                    ));
                    continue;
                }

                match commands::parse(&input) {
                    Err(error) => {
                        let _ = events.send(ChatEvent::Error(error));
                    }
                    Ok(Some(command)) => {
                        handle_command(command, &mut session, &events, &worker_tx, &mut active)
                    }
                    Ok(None) => {
                        session.clear_pending_plan(&events);
                        start_chat(input, &session, &events, &worker_tx, &mut active);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn start_chat(
    user_message: String,
    session: &Session,
    events: &Sender<ChatEvent>,
    worker_tx: &Sender<WorkerResult>,
    active: &mut Option<ActiveRequest>,
) {
    if user_message.len() > MAX_USER_MESSAGE_BYTES {
        let _ = events.send(ChatEvent::Error(format!(
            "message exceeds the {} KiB limit",
            MAX_USER_MESSAGE_BYTES / 1024
        )));
        return;
    }

    let context_message = local_context_message(&session.local_context);
    let context_count = usize::from(context_message.is_some());
    let context_bytes = context_message
        .as_ref()
        .map_or(0, |message| message.content.len());
    if session
        .history
        .len()
        .saturating_add(context_count)
        .saturating_add(2)
        > MAX_HISTORY_MESSAGES
        || history_size(&session.history)
            .saturating_add(context_bytes)
            .saturating_add(user_message.len())
            > MAX_HISTORY_BYTES
    {
        let _ = events.send(ChatEvent::Error(
            "conversation reached its context limit; start a new session or change models."
                .to_owned(),
        ));
        return;
    }

    let mut messages = session.history.clone();
    if let Some(context_message) = context_message {
        let _ = events.send(ChatEvent::Notice(format!(
            "Sending {} requested local result(s) with this message to {}.",
            session.local_context.len(),
            session.connection.profile.provider.display_name()
        )));
        messages.push(context_message);
    }
    messages.push(ChatMessage::new(ChatRole::User, user_message.clone()));
    let profile = session.connection.profile.clone();
    let credential = session.connection.credential.clone();
    let footer = FooterContext {
        profile_id: session.connection.id.clone(),
        base_usage: session.usage,
        turns: session.turns,
    };
    let cancellation = CancellationToken::new();
    let worker_cancellation = cancellation.clone();
    let event_tx = events.clone();
    let result_tx = worker_tx.clone();
    let _ = events.send(ChatEvent::AssistantStarted);
    let thread = thread::spawn(move || {
        let (response, usage) = stream_chat(
            profile,
            credential,
            messages,
            worker_cancellation,
            &event_tx,
            footer,
        );
        let _ = result_tx.send(WorkerResult::Chat { response, usage });
    });
    *active = Some(ActiveRequest {
        cancellation,
        kind: ActiveKind::Chat { user_message },
        thread,
    });
}

/// Session details repeated in footer updates while a response streams.
struct FooterContext {
    profile_id: String,
    base_usage: ChatUsage,
    turns: u64,
}

fn stream_chat(
    profile: AssistantProfile,
    credential: Option<String>,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
    events: &Sender<ChatEvent>,
    footer: FooterContext,
) -> (Result<String, RequestFailure>, TokenUsage) {
    let runtime = match assistant_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            return (
                Err(RequestFailure::Other(format!(
                    "Could not start the assistant runtime: {error}"
                ))),
                TokenUsage::default(),
            );
        }
    };

    let client = match ProviderClient::new() {
        Ok(client) => client,
        Err(error) => {
            return (
                Err(RequestFailure::Other(error.to_string())),
                TokenUsage::default(),
            );
        }
    };

    let mut usage = TokenUsage::default();
    let history_bytes = history_size(&messages);
    let response = runtime.block_on(async {
        let mut stream =
            client.stream_chat(profile.clone(), credential, messages, cancellation.clone());
        let mut response = String::new();
        while let Some(event) = stream.next().await {
            match event {
                Ok(ProviderEvent::TextDelta(delta)) => {
                    if response.len().saturating_add(delta.len()) > MAX_RESPONSE_BYTES
                        || history_bytes
                            .saturating_add(response.len())
                            .saturating_add(delta.len())
                            > MAX_HISTORY_BYTES
                    {
                        return Err(RequestFailure::Other(format!(
                            "The response exceeded the {} KiB conversation limit.\nStart a new session, or ask for a shorter answer.",
                            MAX_RESPONSE_BYTES.min(MAX_HISTORY_BYTES) / 1024
                        )));
                    }
                    response.push_str(&delta);
                    if events.send(ChatEvent::TextDelta(delta)).is_err() {
                        return Err(RequestFailure::Other(
                            "The assistant interface closed.".to_owned(),
                        ));
                    }
                }
                Ok(ProviderEvent::Usage(update)) => {
                    merge_usage(&mut usage, update);
                    let _ = events.send(ChatEvent::Session {
                        profile: footer.profile_id.clone(),
                        provider: profile.provider.display_name().to_owned(),
                        model: profile.model.clone(),
                        effort: switching::effort_label(profile.effort),
                        turns: footer.turns,
                        usage: add_usage(footer.base_usage, usage),
                    });
                }
                Ok(ProviderEvent::Cancelled) => return Err(RequestFailure::Cancelled),
                Err(error) => return Err(RequestFailure::Provider(error)),
            }
        }

        if cancellation.is_cancelled() {
            return Err(RequestFailure::Cancelled);
        }

        if response.trim().is_empty() {
            return Err(RequestFailure::Other(format!(
                "{} returned an empty response.\nTry again, or switch models with `/model ID`.",
                profile.provider.display_name()
            )));
        }

        Ok(response)
    });
    (response, usage)
}

pub(super) fn start_models(
    session: &Session,
    events: &Sender<ChatEvent>,
    worker_tx: &Sender<WorkerResult>,
    active: &mut Option<ActiveRequest>,
) {
    let profile = session.connection.profile.clone();
    let credential = session.connection.credential.clone();
    let cancellation = CancellationToken::new();
    let worker_cancellation = cancellation.clone();
    let result_tx = worker_tx.clone();
    let _ = events.send(ChatEvent::WorkingStarted(
        "Fetching available model IDs…".to_owned(),
    ));
    let thread = thread::spawn(move || {
        let result = (|| {
            let runtime = assistant_runtime().map_err(|error| {
                RequestFailure::Other(format!("Could not start the model-list runtime: {error}"))
            })?;
            runtime.block_on(async {
                let request = Box::pin(list_models(&profile, credential.as_deref()));
                let cancelled = Box::pin(worker_cancellation.cancelled());
                match futures_util::future::select(request, cancelled).await {
                    futures_util::future::Either::Left((result, _)) => {
                        result.map_err(RequestFailure::Provider)
                    }
                    futures_util::future::Either::Right(((), _)) => Err(RequestFailure::Cancelled),
                }
            })
        })();
        let _ = result_tx.send(WorkerResult::Models(result));
    });
    *active = Some(ActiveRequest {
        cancellation,
        kind: ActiveKind::Models,
        thread,
    });
}

pub(super) fn assistant_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn finish_worker(
    result: WorkerResult,
    session: &mut Session,
    events: &Sender<ChatEvent>,
    active: &mut Option<ActiveRequest>,
) {
    let Some(active_request) = active.take() else {
        return;
    };

    let was_cancelled = active_request.cancellation.is_cancelled();
    let _ = active_request.thread.join();
    match (active_request.kind, result) {
        (ActiveKind::Chat { user_message }, WorkerResult::Chat { response, usage }) => {
            session.usage = add_usage(session.usage, usage);
            match response {
                Ok(response) => {
                    if let Some(context_message) = local_context_message(&session.local_context) {
                        session.history.push(context_message);
                        session.local_context.clear();
                        session.local_context_bytes = 0;
                        session.pending_plan_context = None;
                    }
                    session
                        .history
                        .push(ChatMessage::new(ChatRole::User, user_message));
                    session
                        .history
                        .push(ChatMessage::new(ChatRole::Assistant, response));
                    session.turns = session.turns.saturating_add(1);
                    let _ = events.send(ChatEvent::AssistantFinished);
                }
                Err(RequestFailure::Cancelled) => {
                    let _ = events.send(ChatEvent::AssistantCancelled);
                }
                Err(_) if was_cancelled => {
                    let _ = events.send(ChatEvent::AssistantCancelled);
                }
                Err(failure) => {
                    let _ = events.send(ChatEvent::Error(
                        failure.describe(&session.connection.id, &session.connection.profile),
                    ));
                }
            }
            session.send_session(events);
        }
        (ActiveKind::Models, WorkerResult::Models(result)) => {
            let _ = events.send(ChatEvent::WorkingFinished);
            match result {
                Ok(models) if models.is_empty() => {
                    session.known_models.clear();
                    let _ = events.send(ChatEvent::Notice(
                        "The provider returned no model IDs.".to_owned(),
                    ));
                }
                Ok(models) => {
                    session.known_models = models;
                    let shown = session
                        .known_models
                        .iter()
                        .take(MAX_MODEL_SUGGESTIONS)
                        .cloned()
                        .collect::<Vec<_>>();
                    let mut lines =
                        vec![format!("Available models ({})", session.known_models.len())];
                    lines.extend(shown.iter().cloned());
                    if session.known_models.len() > shown.len() {
                        lines.push(format!(
                            "… {} more; Tab completes up to {} matching IDs",
                            session.known_models.len() - shown.len(),
                            MAX_MODEL_SUGGESTIONS
                        ));
                    }

                    let result = lines.join("\n");
                    let _ = session.queue_local_context(&result, events);
                    let _ = events.send(ChatEvent::AssistantMessage(result));
                    session.send_completions("/model ", events);
                }
                Err(RequestFailure::Cancelled) => {
                    let _ = events.send(ChatEvent::Notice("Model listing cancelled.".to_owned()));
                }
                Err(_) if was_cancelled => {
                    let _ = events.send(ChatEvent::Notice("Model listing cancelled.".to_owned()));
                }
                Err(failure) => {
                    let _ = events.send(ChatEvent::Error(
                        failure.describe(&session.connection.id, &session.connection.profile),
                    ));
                }
            }
        }
        _ => {
            let _ = events.send(ChatEvent::Error(
                "assistant worker returned an unexpected result".to_owned(),
            ));
        }
    }
}

pub(super) fn merge_usage(total: &mut TokenUsage, update: TokenUsage) {
    if update.input_tokens.is_some() {
        total.input_tokens = update.input_tokens;
    }
    if update.output_tokens.is_some() {
        total.output_tokens = update.output_tokens;
    }
    if update.total_tokens.is_some() {
        total.total_tokens = update.total_tokens;
    }
}

pub(super) fn add_usage(mut total: ChatUsage, update: TokenUsage) -> ChatUsage {
    fn add(current: &mut Option<u64>, increment: Option<u64>) {
        if let Some(increment) = increment {
            *current = Some((*current).unwrap_or_default().saturating_add(increment));
        }
    }
    add(&mut total.input_tokens, update.input_tokens);
    add(&mut total.output_tokens, update.output_tokens);
    add(&mut total.total_tokens, update.total_tokens);
    total
}

#[cfg(test)]
mod tests {
    use super::assistant_runtime;

    #[test]
    fn worker_runtime_enables_network_io() {
        assistant_runtime().unwrap().block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            assert_ne!(listener.local_addr().unwrap().port(), 0);
        });
    }
}
