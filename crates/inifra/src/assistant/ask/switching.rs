//! Changing the session's profile, model, and effort, and saving them as defaults.

use super::{
    ActiveConnection, Session, failure,
    worker::{RuntimeError, WorkerResult, assistant_runtime},
};
use inifra_assistant::{
    AssistantConfig, AssistantProfile, Effort, list_models, resolve_credential,
};
use inifra_tui::{ChatEvent, ModelList, SwitchRequest, SwitcherProfile};
use std::{sync::mpsc::Sender, thread};

const DEFAULT_EFFORT_LABEL: &str = "provider default";

/// Effort labels offered by the switcher; the first means "send no effort setting".
pub(super) fn effort_labels() -> Vec<String> {
    std::iter::once(DEFAULT_EFFORT_LABEL)
        .chain(Effort::ALL.iter().map(|effort| effort.as_str()))
        .map(str::to_owned)
        .collect()
}

pub(super) fn effort_label(effort: Option<Effort>) -> Option<String> {
    effort.map(|effort| effort.as_str().to_owned())
}

pub(super) fn open_switcher(session: &Session, events: &Sender<ChatEvent>) {
    let profiles = session
        .profiles
        .iter()
        .map(|(id, profile)| SwitcherProfile {
            id: id.clone(),
            detail: format!(
                "{} · {}",
                profile.provider.display_name(),
                profile.api.short_name()
            ),
            default_model: profile.model.clone(),
        })
        .collect();
    let _ = events.send(ChatEvent::SwitcherOpened {
        profiles,
        active: session.connection.id.clone(),
        model: session.connection.profile.model.clone(),
        efforts: effort_labels(),
        effort: effort_label(session.connection.profile.effort),
    });
}

/// Starts fetching model IDs for one profile; the result arrives as `WorkerResult::SwitcherModels`.
pub(super) fn load_models(
    profile_id: String,
    session: &Session,
    events: &Sender<ChatEvent>,
    worker_tx: &Sender<WorkerResult>,
) {
    if profile_id == session.connection.id && !session.known_models.is_empty() {
        let _ = events.send(ChatEvent::ModelsListed {
            profile: profile_id,
            models: ModelList::Ready(session.known_models.clone()),
        });
        return;
    }

    let (profile, credential) = if profile_id == session.connection.id {
        (
            session.connection.profile.clone(),
            session.connection.credential.clone(),
        )
    } else {
        let Some(profile) = session.profiles.get(&profile_id).cloned() else {
            let _ = events.send(ChatEvent::ModelsListed {
                profile: profile_id,
                models: ModelList::Unavailable("This profile no longer exists.".to_owned()),
            });
            return;
        };
        match resolve_credential(&profile, &session.credentials_path, |name| {
            std::env::var(name).ok()
        }) {
            Ok(credential) => (profile, credential),
            Err(_) => {
                let reason = match &profile.api_key_env {
                    Some(name) => format!("{name} is not set, so this profile can't be used yet."),
                    None => "This profile's credential could not be loaded.".to_owned(),
                };
                let _ = events.send(ChatEvent::ModelsListed {
                    profile: profile_id,
                    models: ModelList::Unavailable(reason),
                });
                return;
            }
        }
    };

    let result_tx = worker_tx.clone();
    thread::spawn(move || {
        let result = assistant_runtime()
            .map_err(RuntimeError::from)
            .and_then(|runtime| {
                runtime
                    .block_on(list_models(&profile, credential.as_deref()))
                    .map_err(RuntimeError::Provider)
            });
        let _ = result_tx.send(WorkerResult::SwitcherModels {
            profile: profile_id,
            result,
        });
    });
}

pub(super) fn finish_models(
    profile_id: String,
    result: Result<Vec<String>, RuntimeError>,
    session: &mut Session,
    events: &Sender<ChatEvent>,
) {
    let models = match result {
        Ok(models) => {
            if profile_id == session.connection.id {
                session.known_models = models.clone();
            }
            ModelList::Ready(models)
        }
        Err(RuntimeError::Runtime(message)) => ModelList::Unavailable(message),
        Err(RuntimeError::Provider(error)) => {
            let summary = session
                .profiles
                .get(&profile_id)
                .map(|profile| failure::describe_provider_error(&error, &profile_id, profile))
                .unwrap_or_else(|| error.to_string());
            ModelList::Unavailable(format!(
                "Model IDs unavailable: {}",
                summary.lines().next().unwrap_or_default()
            ))
        }
    };
    let _ = events.send(ChatEvent::ModelsListed {
        profile: profile_id,
        models,
    });
}

/// Makes `id` the active profile, resetting the conversation. Reports a failure and returns
/// `false` when the profile is unknown or its credential is unavailable.
pub(super) fn activate_profile(
    id: &str,
    session: &mut Session,
    events: &Sender<ChatEvent>,
) -> bool {
    let Some(profile) = session.profiles.get(id).cloned() else {
        let _ = events.send(ChatEvent::Error(format!(
            "No profile named {id}.\nList profiles with `/profile`, or create one with `inifra assistant`."
        )));
        return false;
    };
    let credential = match resolve_credential(&profile, &session.credentials_path, |name| {
        std::env::var(name).ok()
    }) {
        Ok(credential) => credential,
        Err(error) => {
            let _ = events.send(ChatEvent::Error(failure::describe_switch_failure(
                id,
                &profile,
                &session.connection.id,
                &error.to_string(),
            )));
            return false;
        }
    };

    session.clear_pending_plan(events);
    session.clear_local_context();
    session.history.truncate(1);
    session.known_models.clear();
    session.connection = ActiveConnection {
        id: id.to_owned(),
        profile,
        credential,
    };
    true
}

/// Applies a switcher selection and, when asked, saves it as the default.
pub(super) fn apply_switch(
    request: SwitchRequest,
    session: &mut Session,
    events: &Sender<ChatEvent>,
) {
    let effort = request.effort.as_deref().and_then(Effort::parse);
    let profile_changed = request.profile != session.connection.id;
    if profile_changed && !activate_profile(&request.profile, session, events) {
        return;
    }

    let model_changed = session.connection.profile.model != request.model;
    if model_changed && !profile_changed {
        session.history.truncate(1);
        session.clear_local_context();
    }
    session.connection.profile.model = request.model;
    session.connection.profile.effort = effort;
    session.send_session(events);

    let mut notice = format!("Now using {}.", connection_summary(&session.connection));
    if profile_changed || model_changed {
        notice.push_str(" Conversation context was cleared.");
    }
    let _ = events.send(ChatEvent::Notice(notice));

    if request.save_default {
        save_as_default(session, events);
    }
}

/// Sets the session effort from `/effort`; `None` shows the current value.
pub(super) fn set_effort(value: Option<&str>, session: &mut Session, events: &Sender<ChatEvent>) {
    let Some(value) = value else {
        let current = effort_label(session.connection.profile.effort)
            .unwrap_or_else(|| DEFAULT_EFFORT_LABEL.to_owned());
        let _ = events.send(ChatEvent::Notice(format!(
            "Effort is {current}. Change it with `/effort low|medium|high|default`."
        )));
        return;
    };

    let effort = if value.eq_ignore_ascii_case("default") {
        None
    } else {
        match Effort::parse(value) {
            Some(effort) => Some(effort),
            None => {
                let _ = events.send(ChatEvent::Error(format!(
                    "Unknown effort {value}.\nUse `/effort low`, `medium`, `high`, or `default`."
                )));
                return;
            }
        }
    };
    session.connection.profile.effort = effort;
    session.send_session(events);
    let _ = events.send(ChatEvent::Notice(match effort {
        Some(effort) => format!("Effort set to {} for this session.", effort.as_str()),
        None => "Effort reset to the provider default for this session.".to_owned(),
    }));
}

fn connection_summary(connection: &ActiveConnection) -> String {
    let profile = &connection.profile;
    let effort = effort_label(profile.effort)
        .map(|effort| format!(" · {effort} effort"))
        .unwrap_or_default();
    format!(
        "{} · {} · {}{effort}",
        connection.id,
        profile.provider.display_name(),
        profile.model
    )
}

fn save_as_default(session: &mut Session, events: &Sender<ChatEvent>) {
    let result = (|| -> Result<AssistantProfile, Box<dyn std::error::Error>> {
        let mut config =
            AssistantConfig::load(&session.config_path)?.ok_or("assistant.toml is missing")?;
        let id = &session.connection.id;
        let mut saved = config
            .profile(id)
            .cloned()
            .ok_or_else(|| format!("profile {id} is no longer saved"))?;
        saved.model = session.connection.profile.model.clone();
        saved.effort = session.connection.profile.effort;
        config.replace_profile(id, saved.clone())?;
        config.set_default_profile(id)?;
        config.save(&session.config_path)?;
        Ok(saved)
    })();

    match result {
        Ok(saved) => {
            session
                .profiles
                .insert(session.connection.id.clone(), saved);
            let _ = events.send(ChatEvent::Notice(format!(
                "Saved {} as the default profile, with this model and effort.",
                session.connection.id
            )));
        }
        Err(error) => {
            let _ = events.send(ChatEvent::Error(format!(
                "Could not save the default profile.\nThe session still uses this selection.\ndetails: {error}"
            )));
        }
    }
}
