use super::UiExit;
use inifra_assistant::{
    ApiProtocol, AssistantConfig, AssistantProfile, ConnectionTemplate, CredentialSource, Effort,
    PROVIDER_PRESETS, Provider, ProviderPreset, connection_template, is_valid_profile_id,
    list_models, read_credential, write_credential,
};
use inifra_tui::{
    Badge, FormOutcome, Heading, InteractionOutcome, OutlineRow, Row, SelectScreen, StepState,
    TextField, presentation, presentation::Tone, run_form, run_secret_input, run_select,
};
use std::{collections::BTreeMap, error::Error, fs, io::Write, path::Path};

pub(super) fn configure_profile(
    config_path: &Path,
    credentials_path: &Path,
    mut config: AssistantConfig,
    profile_id: Option<&str>,
) -> Result<UiExit, Box<dyn Error>> {
    let presets = PROVIDER_PRESETS
        .iter()
        .filter(|preset| preset.selectable)
        .copied()
        .collect::<Vec<_>>();
    let existing = profile_id.and_then(|id| config.profile(id)).cloned();
    let mut setup = Setup {
        choice: ConnectionChoice::initial(existing.as_ref(), &presets),
        config: &config,
        credentials_path,
        profile_id,
        existing,
        presets,
        single_step: false,
    };

    let mut draft = Draft::default();
    let mut index = 0;
    loop {
        let steps = setup.steps(&draft);
        let step = steps[index.min(steps.len() - 1)];
        let flow = match step {
            Step::Connection => setup.connection(&mut draft)?,
            Step::Name => setup.name(&mut draft)?,
            Step::BaseUrl => setup.base_url(&mut draft)?,
            Step::Key => setup.key(&mut draft)?,
            Step::Model => setup.model(&mut draft)?,
            Step::Effort => setup.effort(&mut draft)?,
        };
        match flow {
            Flow::Quit => return Ok(UiExit::Quit),
            Flow::Next if step == Step::Effort => break,
            Flow::Next => index += 1,
            Flow::Back if index == 0 => return Ok(UiExit::Continue),
            Flow::Back => {
                index -= 1;
                if steps[index] == Step::Connection {
                    setup.choice.resume_at_list = true;
                }
            }
        }
    }

    let id = setup.id(&draft)?;
    let (profile, key) = build_profile(&mut draft)?;
    save_profile(
        config_path,
        credentials_path,
        &mut config,
        &id,
        profile,
        &key,
    )?;
    let saved_profile = config
        .profile(&id)
        .ok_or("saved assistant profile is missing from the configuration")?;
    println!("{}", saved_message(&id, saved_profile));
    println!("{}", default_message(&config, &id));
    Ok(UiExit::Continue)
}

/// Turns the answers into a validated profile and the key setup that goes with it.
fn build_profile(draft: &mut Draft) -> Result<(AssistantProfile, KeySetup), Box<dyn Error>> {
    let template = draft.template.ok_or("no connection was chosen")?;
    let key = draft.key.take().ok_or("no API key choice was made")?;
    let profile = AssistantProfile {
        provider: template.provider,
        api: template.api,
        model: draft.model.take().ok_or("no model was chosen")?,
        base_url: draft.base_url.clone().filter(|value| !value.is_empty()),
        api_key_env: key.api_key_env.clone(),
        credential_source: key.source,
        effort: draft.effort.as_deref().and_then(Effort::parse),
    };
    profile.validate()?;
    Ok((profile, key))
}

/// Writes the profile and any pasted key; a failed key write restores the previous config.
fn save_profile(
    config_path: &Path,
    credentials_path: &Path,
    config: &mut AssistantConfig,
    id: &str,
    profile: AssistantProfile,
    key: &KeySetup,
) -> Result<(), Box<dyn Error>> {
    let original = AssistantConfig::load(config_path)?;
    if config.profile(id).is_some() {
        config.replace_profile(id, profile)?;
    } else {
        config.insert_profile(id, profile)?;
    }
    config.save(config_path)?;

    if let (Some(name), Some(secret)) = (key.api_key_env.as_deref(), key.secret.as_deref())
        && let Err(error) = write_credential(credentials_path, name, secret)
    {
        restore_config(config_path, original)?;
        return Err(error.into());
    }
    Ok(())
}

/// The parts of a saved profile that can be edited on their own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Part {
    Model,
    Key,
    Effort,
    BaseUrl,
    Connection,
}

/// Lists a profile's parts with their current values and edits only the one that is chosen.
/// Choosing the connection re-runs the full setup, because everything else depends on it.
pub(super) fn edit_profile(
    config_path: &Path,
    credentials_path: &Path,
    id: &str,
) -> Result<UiExit, Box<dyn Error>> {
    let mut last = Part::Model;
    loop {
        let mut config = AssistantConfig::load(config_path)?.unwrap_or_default();
        let Some(profile) = config.profile(id).cloned() else {
            return Ok(UiExit::Continue);
        };
        let template = existing_template(&profile)?;
        let parts = edit_parts(template, &profile);
        let screen = SelectScreen {
            heading: Heading::new("edit profile").context(id),
            choices: parts.iter().map(|(_, row)| row.clone()).collect(),
            initial: parts
                .iter()
                .position(|(part, _)| *part == last)
                .unwrap_or_default(),
            ..SelectScreen::default()
        };
        let part = match run_select(&screen)? {
            InteractionOutcome::Selected(selected) => parts[selected].0,
            InteractionOutcome::Back => return Ok(UiExit::Continue),
            InteractionOutcome::Quit => return Ok(UiExit::Quit),
        };
        last = part;

        if part == Part::Connection {
            if configure_profile(config_path, credentials_path, config, Some(id))? == UiExit::Quit {
                return Ok(UiExit::Quit);
            }
            continue;
        }

        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let mut setup = Setup {
            choice: ConnectionChoice::initial(Some(&profile), &presets),
            config: &config,
            credentials_path,
            profile_id: Some(id),
            existing: Some(profile.clone()),
            presets,
            single_step: true,
        };
        let mut draft = Draft::from_profile(template, id, &profile);
        let flow = match part {
            Part::Model => setup.model(&mut draft)?,
            Part::Key => setup.key(&mut draft)?,
            Part::Effort => setup.effort(&mut draft)?,
            Part::BaseUrl => setup.base_url(&mut draft)?,
            Part::Connection => unreachable!("handled above"),
        };
        match flow {
            Flow::Quit => return Ok(UiExit::Quit),
            Flow::Back => {}
            Flow::Next => {
                let (updated, key) = build_profile(&mut draft)?;
                save_profile(
                    config_path,
                    credentials_path,
                    &mut config,
                    id,
                    updated,
                    &key,
                )?;
                let saved = config
                    .profile(id)
                    .ok_or("saved assistant profile is missing from the configuration")?;
                println!("{}", saved_message(id, saved));
            }
        }
    }
}

/// The template a saved profile was made from; legacy OpenAI-compatible profiles count as
/// custom endpoints.
fn existing_template(profile: &AssistantProfile) -> Result<ConnectionTemplate, Box<dyn Error>> {
    let provider = if profile.provider == Provider::OpenAiCompatible {
        Provider::CustomEndpoint
    } else {
        profile.provider
    };
    connection_template(provider, profile.api)
        .ok_or_else(|| "this profile uses a connection that is no longer supported".into())
}

/// The editable parts for a profile, in the order they are offered, with current values.
fn edit_parts(template: ConnectionTemplate, profile: &AssistantProfile) -> Vec<(Part, Row)> {
    let mut parts = vec![(Part::Model, Row::new("Model").detail(profile.model.clone()))];
    if key_possible(template, Some(profile)) {
        parts.push((Part::Key, Row::new("API key").detail(key_summary(profile))));
    }
    parts.push((
        Part::Effort,
        Row::new("Effort").detail(
            profile
                .effort
                .map_or("provider default", |effort| effort.as_str())
                .to_owned(),
        ),
    ));
    if let Some(base_url) = &profile.base_url {
        parts.push((
            Part::BaseUrl,
            Row::new("API base URL").detail(base_url.clone()),
        ));
    }
    parts.push((
        Part::Connection,
        Row::new("Connection").detail(format!(
            "{} · {}",
            profile.provider.display_name(),
            profile.api.short_name()
        )),
    ));
    parts
}

/// How a saved profile gets its key, without ever including the key.
pub(super) fn key_summary(profile: &AssistantProfile) -> String {
    match (&profile.api_key_env, profile.credential_source) {
        (None, _) => "no key".to_owned(),
        (Some(name), CredentialSource::Environment) => format!("{name} (environment)"),
        (Some(name), CredentialSource::CredentialsFile) => format!("{name} (stored)"),
    }
}

/// The setup screens. Each can be revisited with Esc without losing earlier answers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Connection,
    Name,
    BaseUrl,
    Key,
    Model,
    Effort,
}

enum Flow {
    Next,
    Back,
    Quit,
}

/// What the user has chosen so far.
#[derive(Default)]
struct Draft {
    template: Option<ConnectionTemplate>,
    name: Option<String>,
    base_url: Option<String>,
    key: Option<KeySetup>,
    /// The key option last picked, so stepping back to the key screen lands on it again.
    key_choice: Option<KeyChoice>,
    model: Option<String>,
    effort: Option<String>,
}

impl Draft {
    /// Answers that match a saved profile, so one step can be redone and the rest kept.
    fn from_profile(template: ConnectionTemplate, id: &str, profile: &AssistantProfile) -> Self {
        Self {
            template: Some(template),
            name: Some(id.to_owned()),
            base_url: profile.base_url.clone(),
            key: Some(KeySetup {
                api_key_env: profile.api_key_env.clone(),
                source: profile.credential_source,
                secret: None,
            }),
            key_choice: None,
            model: Some(profile.model.clone()),
            effort: profile.effort.map(|effort| effort.as_str().to_owned()),
        }
    }
}

struct KeySetup {
    api_key_env: Option<String>,
    source: CredentialSource,
    secret: Option<String>,
}

impl KeySetup {
    /// One line for the outline; never includes the key itself.
    fn summary(&self) -> String {
        match (&self.api_key_env, self.source, &self.secret) {
            (None, _, _) => "no key".to_owned(),
            (Some(name), CredentialSource::CredentialsFile, Some(_)) => {
                format!("pasted · {name}")
            }
            (Some(name), CredentialSource::CredentialsFile, None) => format!("stored · {name}"),
            (Some(name), CredentialSource::Environment, _) => format!("{name} (environment)"),
        }
    }
}

struct Setup<'a> {
    choice: ConnectionChoice,
    config: &'a AssistantConfig,
    credentials_path: &'a Path,
    profile_id: Option<&'a str>,
    existing: Option<AssistantProfile>,
    presets: Vec<ProviderPreset>,
    /// Editing one part of a saved profile: screens show no outline of other steps.
    single_step: bool,
}

impl Setup<'_> {
    /// The steps this connection needs, in order. Steps with nothing to ask are left out.
    fn steps(&self, draft: &Draft) -> Vec<Step> {
        let mut steps = vec![Step::Connection];
        if self.profile_id.is_none() {
            steps.push(Step::Name);
        }
        if let Some(template) = draft.template {
            if !endpoint_fields(template, self.existing.as_ref()).is_empty() {
                steps.push(Step::BaseUrl);
            }
            if key_possible(template, self.existing.as_ref()) {
                steps.push(Step::Key);
            }
        } else {
            steps.push(Step::Key);
        }
        steps.extend([Step::Model, Step::Effort]);
        steps
    }

    /// Every step as an outline row: finished ones with their answers, the current one marked,
    /// and upcoming ones pending. `branch` names a sub-step taken under the current step.
    fn outline_rows(
        &self,
        draft: &Draft,
        current: Step,
        branch: Option<(&str, Option<String>)>,
    ) -> Vec<OutlineRow> {
        let steps = self.steps(draft);
        let current_index = steps
            .iter()
            .position(|step| *step == current)
            .unwrap_or_default();
        let mut rows = Vec::new();
        let mut row = |step: Step, label: &str, value: Option<String>| {
            let position = steps.iter().position(|candidate| *candidate == step);
            let state = match position {
                None => StepState::Done,
                Some(position) if position < current_index => StepState::Done,
                Some(position) if position == current_index => StepState::Active,
                Some(_) => StepState::Pending,
            };
            let mut outline = OutlineRow::new(label, state);
            if let Some(value) = value {
                outline = outline.value(value);
            }
            if step == current
                && let Some((name, detail)) = &branch
            {
                outline = outline.branch(*name, detail.clone());
            }
            rows.push(outline);
        };
        row(
            Step::Connection,
            "Connection",
            draft.template.map(|template| {
                format!(
                    "{} · {}",
                    template.provider.display_name(),
                    template.api.short_name()
                )
            }),
        );
        row(
            Step::Name,
            "Profile name",
            self.profile_id
                .map(str::to_owned)
                .or_else(|| draft.name.clone()),
        );
        if steps.contains(&Step::BaseUrl) {
            row(Step::BaseUrl, "API base URL", draft.base_url.clone());
        }
        if steps.contains(&Step::Key) {
            row(
                Step::Key,
                "API key",
                draft.key.as_ref().map(KeySetup::summary),
            );
        }
        row(Step::Model, "Default model", draft.model.clone());
        row(
            Step::Effort,
            "Default effort",
            draft.effort.as_ref().map(|effort| {
                if effort.is_empty() {
                    "provider default".to_owned()
                } else {
                    effort.clone()
                }
            }),
        );
        rows
    }

    /// The heading with every step listed beneath it, so progress stays visible.
    fn heading(
        &self,
        draft: &Draft,
        current: Step,
        branch: Option<(&str, Option<String>)>,
    ) -> Heading {
        if self.single_step {
            let part = match current {
                Step::Connection => "connection",
                Step::Name => "name",
                Step::BaseUrl => "API base URL",
                Step::Key => "API key",
                Step::Model => "model",
                Step::Effort => "effort",
            };
            let mut heading = Heading::new("edit profile")
                .context(self.profile_id.unwrap_or_default())
                .context(part);
            if let Some((name, _)) = branch {
                heading = heading.context(name);
            }
            return heading;
        }
        Heading::new(if self.existing.is_some() {
            "edit profile"
        } else {
            "new profile"
        })
        .outline(self.outline_rows(draft, current, branch))
    }

    fn id(&self, draft: &Draft) -> Result<String, Box<dyn Error>> {
        match (self.profile_id, &draft.name) {
            (Some(id), _) => Ok(id.to_owned()),
            (None, Some(name)) => Ok(name.clone()),
            (None, None) => Err("no profile name was entered".into()),
        }
    }

    fn template(draft: &Draft) -> Result<ConnectionTemplate, Box<dyn Error>> {
        draft
            .template
            .ok_or_else(|| "no connection was chosen".into())
    }

    fn connection(&mut self, draft: &mut Draft) -> Result<Flow, Box<dyn Error>> {
        let heading = self.heading(draft, Step::Connection, None);
        let template = match select_connection(
            &heading,
            &mut self.choice,
            &self.presets,
            self.existing.as_ref(),
        )? {
            Selected::Template(template) => template,
            Selected::Back => return Ok(Flow::Back),
            Selected::Quit => return Ok(Flow::Quit),
        };
        if draft.template != Some(template) {
            *draft = Draft {
                template: Some(template),
                name: draft.name.take(),
                ..Draft::default()
            };
        }
        Ok(Flow::Next)
    }

    fn name(&mut self, draft: &mut Draft) -> Result<Flow, Box<dyn Error>> {
        let field = profile_id_field(self.config).default_value(draft.name.clone());
        match run_form(&self.heading(draft, Step::Name, None), vec![field])? {
            FormOutcome::Submitted(values) => {
                draft.name = Some(normalize_profile_name(&required_value(&values, "id")?));
                Ok(Flow::Next)
            }
            FormOutcome::Back => Ok(Flow::Back),
            FormOutcome::Quit => Ok(Flow::Quit),
        }
    }

    fn base_url(&mut self, draft: &mut Draft) -> Result<Flow, Box<dyn Error>> {
        let template = Self::template(draft)?;
        let Some(field) = endpoint_fields(template, self.existing.as_ref())
            .into_iter()
            .next()
        else {
            return Ok(Flow::Next);
        };
        let default = draft.base_url.clone().or_else(|| field.default.clone());
        let field = field.default_value(default);
        match run_form(&self.heading(draft, Step::BaseUrl, None), vec![field])? {
            FormOutcome::Submitted(values) => {
                draft.base_url = Some(required_value(&values, "base_url")?);
                Ok(Flow::Next)
            }
            FormOutcome::Back => Ok(Flow::Back),
            FormOutcome::Quit => Ok(Flow::Quit),
        }
    }

    fn key(&mut self, draft: &mut Draft) -> Result<Flow, Box<dyn Error>> {
        let template = Self::template(draft)?;
        let id = self.id(draft)?;
        if !key_possible(template, self.existing.as_ref()) {
            draft.key = Some(KeySetup {
                api_key_env: None,
                source: CredentialSource::Environment,
                secret: None,
            });
            return Ok(Flow::Next);
        }

        let name = key_variable_name(template, &id, self.existing.as_ref());
        let keep_stored = self.existing.as_ref().is_some_and(|profile| {
            profile.credential_source == CredentialSource::CredentialsFile
                && profile.api_key_env.as_deref() == Some(&name)
        }) && read_credential(self.credentials_path, &name)?
            .is_some_and(|secret| !secret.trim().is_empty());
        let options = key_options(template, &name, keep_stored);
        let mut last_choice = draft.key_choice;
        loop {
            let initial = last_choice
                .or_else(|| existing_key_choice(self.existing.as_ref(), keep_stored))
                .unwrap_or(if template.provider == Provider::Ollama {
                    KeyChoice::None
                } else {
                    KeyChoice::Paste
                });
            let screen = SelectScreen {
                heading: self.heading(draft, Step::Key, None),
                choices: options.iter().map(|(_, row)| row.clone()).collect(),
                initial: options
                    .iter()
                    .position(|(kind, _)| *kind == initial)
                    .unwrap_or_default(),
                ..SelectScreen::default()
            };
            let selected = match run_select(&screen)? {
                InteractionOutcome::Selected(selected) => options[selected].0,
                InteractionOutcome::Back => return Ok(Flow::Back),
                InteractionOutcome::Quit => return Ok(Flow::Quit),
            };
            last_choice = Some(selected);
            draft.key_choice = Some(selected);

            let setup = match selected {
                KeyChoice::None => KeySetup {
                    api_key_env: None,
                    source: CredentialSource::Environment,
                    secret: None,
                },
                KeyChoice::Variable => KeySetup {
                    api_key_env: Some(name.clone()),
                    source: CredentialSource::Environment,
                    secret: None,
                },
                KeyChoice::Keep => KeySetup {
                    api_key_env: Some(name.clone()),
                    source: CredentialSource::CredentialsFile,
                    secret: None,
                },
                KeyChoice::OtherVariable => {
                    let field = TextField::new("name", "Variable name")
                        .required()
                        .hint("the environment variable's name, such as WORK_API_KEY")
                        .default_value(Some(name.clone()))
                        .check(|value| {
                            if is_variable_name(value) {
                                Ok(())
                            } else {
                                Err("Use uppercase letters, digits, and underscores.".to_owned())
                            }
                        });
                    match run_form(
                        &self.heading(
                            draft,
                            Step::Key,
                            Some(("Variable name", Some("an environment variable".to_owned()))),
                        ),
                        vec![field],
                    )? {
                        FormOutcome::Submitted(values) => KeySetup {
                            api_key_env: Some(required_value(&values, "name")?),
                            source: CredentialSource::Environment,
                            secret: None,
                        },
                        FormOutcome::Back => continue,
                        FormOutcome::Quit => return Ok(Flow::Quit),
                    }
                }
                KeyChoice::Paste => {
                    let field = TextField::new("secret", "API key").hint(format!(
                        "saved as {name} in credentials.env, never in assistant.toml"
                    ));
                    match run_secret_input(
                        &self.heading(
                            draft,
                            Step::Key,
                            Some((
                                "Paste a key",
                                Some(format!("saved as {name} in credentials.env")),
                            )),
                        ),
                        field,
                    )? {
                        FormOutcome::Submitted(values) => KeySetup {
                            api_key_env: Some(name.clone()),
                            source: CredentialSource::CredentialsFile,
                            secret: Some(required_value(&values, "secret")?),
                        },
                        FormOutcome::Back => continue,
                        FormOutcome::Quit => return Ok(Flow::Quit),
                    }
                }
            };
            draft.key = Some(setup);
            return Ok(Flow::Next);
        }
    }

    /// Offers the provider's own model list when it can be fetched, and a typed ID otherwise.
    fn model(&mut self, draft: &mut Draft) -> Result<Flow, Box<dyn Error>> {
        let template = Self::template(draft)?;
        let heading = self.heading(draft, Step::Model, None);
        let previous = draft
            .model
            .clone()
            .or_else(|| self.existing.as_ref().map(|profile| profile.model.clone()));

        let (models, note) = match self.fetch_models(template, draft, previous.as_deref()) {
            Ok(models) if !models.is_empty() => (models, None),
            Ok(_) => (Vec::new(), Some("the provider listed no models".to_owned())),
            Err(reason) => (Vec::new(), Some(reason)),
        };

        loop {
            if !models.is_empty() {
                let mut choices = vec![Row::new("Enter a model ID").detail("type one yourself")];
                choices.extend(models.iter().map(Row::new));
                let initial = previous
                    .as_deref()
                    .and_then(|model| models.iter().position(|candidate| candidate == model))
                    .map_or(1, |position| position + 1);
                let screen = SelectScreen {
                    heading: heading.clone(),
                    choices,
                    initial,
                    filterable: true,
                    ..SelectScreen::default()
                };
                match run_select(&screen)? {
                    InteractionOutcome::Selected(0) => {}
                    InteractionOutcome::Selected(selected) => {
                        draft.model = Some(models[selected - 1].clone());
                        return Ok(Flow::Next);
                    }
                    InteractionOutcome::Back => return Ok(Flow::Back),
                    InteractionOutcome::Quit => return Ok(Flow::Quit),
                }
            }

            let hint = match &note {
                Some(reason) => {
                    format!("{reason} · switch for one session with /model in inifra ask")
                }
                None => "switch for one session with /model in inifra ask".to_owned(),
            };
            let field = TextField::new("model", "Default model")
                .required()
                .hint(hint)
                .default_value(previous.clone());
            let typed_heading = if models.is_empty() {
                heading.clone()
            } else {
                self.heading(
                    draft,
                    Step::Model,
                    Some(("Model ID", Some("typed".to_owned()))),
                )
            };
            match run_form(&typed_heading, vec![field])? {
                FormOutcome::Submitted(values) => {
                    draft.model = Some(required_value(&values, "model")?);
                    return Ok(Flow::Next);
                }
                FormOutcome::Back if models.is_empty() => return Ok(Flow::Back),
                FormOutcome::Back => {}
                FormOutcome::Quit => return Ok(Flow::Quit),
            }
        }
    }

    fn effort(&mut self, draft: &mut Draft) -> Result<Flow, Box<dyn Error>> {
        let default = draft.effort.clone().or_else(|| {
            self.existing
                .as_ref()
                .and_then(|profile| profile.effort)
                .map(|effort| effort.as_str().to_owned())
        });
        let heading = self.heading(draft, Step::Effort, None);
        match run_form(&heading, vec![effort_field(default)])? {
            FormOutcome::Submitted(values) => {
                draft.effort = values.get("effort").cloned();
                Ok(Flow::Next)
            }
            FormOutcome::Back => Ok(Flow::Back),
            FormOutcome::Quit => Ok(Flow::Quit),
        }
    }

    /// Asks the provider for its model IDs with the key just chosen. Any failure is returned as
    /// a short reason so the caller can fall back to typing a model ID.
    fn fetch_models(
        &self,
        template: ConnectionTemplate,
        draft: &Draft,
        model: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let key = draft.key.as_ref().ok_or("no API key choice was made")?;
        let credential = match (&key.secret, &key.api_key_env, key.source) {
            (Some(secret), _, _) => Some(secret.clone()),
            (None, Some(name), CredentialSource::Environment) => std::env::var(name).ok(),
            (None, Some(name), CredentialSource::CredentialsFile) => {
                read_credential(self.credentials_path, name).ok().flatten()
            }
            (None, None, _) => None,
        };
        let profile = AssistantProfile {
            provider: template.provider,
            api: template.api,
            model: model.unwrap_or("model").to_owned(),
            base_url: draft.base_url.clone().filter(|value| !value.is_empty()),
            api_key_env: key.api_key_env.clone(),
            credential_source: key.source,
            effort: None,
        };

        let mut stderr = std::io::stderr();
        let _ = write!(stderr, "Looking up available models…");
        let _ = stderr.flush();
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("could not start the model lookup: {error}"))
            .and_then(|runtime| {
                runtime
                    .block_on(list_models(&profile, credential.as_deref()))
                    .map_err(|error| short_reason(&error.to_string()))
            });
        let _ = write!(stderr, "\r\x1b[2K");
        let _ = stderr.flush();
        result
    }
}

/// A short reason shown when the model list cannot be fetched. The provider's own error text is
/// long and rarely actionable here, so only the cases a user can fix are named.
fn short_reason(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("api key is missing") {
        "the model list needs an API key".to_owned()
    } else if lower.contains("401") || lower.contains("403") || lower.contains("unauthorized") {
        "the provider rejected the API key".to_owned()
    } else {
        "could not fetch the model list".to_owned()
    }
}

/// Profile names are stored in lowercase, so `Work` and `work` are the same name.
fn normalize_profile_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn profile_id_field(config: &AssistantConfig) -> TextField {
    let taken = config.profiles().keys().cloned().collect::<Vec<_>>();
    TextField::new("id", "Profile name")
        .required()
        .hint("a short name, like work or local · letters, numbers, hyphens · saved in lowercase")
        .check(move |name| {
            let id = normalize_profile_name(name);
            if !is_valid_profile_id(&id) {
                Err("Use letters, numbers, and hyphens, starting with a letter.".to_owned())
            } else if taken.contains(&id) {
                Err(format!(
                    "A profile named {id} already exists. Choose another name, or edit it from the profile list."
                ))
            } else {
                Ok(())
            }
        })
}

/// Summarizes what a provider preset configures: protocol, endpoint host, and credential.
fn preset_detail(preset: ProviderPreset) -> String {
    let mut parts = vec![preset.default_api.short_name().to_owned()];
    if let Some(host) = preset.base_url.map(url_host) {
        parts.push(host.to_owned());
    }
    if preset.api_key_env.is_none() {
        parts.push("no key".to_owned());
    }
    parts.join(" · ")
}

/// Names the first few providers and counts the rest, keeping the row on one line.
fn known_provider_detail(presets: &[ProviderPreset]) -> String {
    const SHOWN: usize = 3;
    let names = presets
        .iter()
        .take(SHOWN)
        .map(|preset| preset.provider.display_name())
        .collect::<Vec<_>>()
        .join(", ");
    match presets.len().saturating_sub(SHOWN) {
        0 => names,
        more => format!("{names} +{more} more"),
    }
}

fn custom_endpoint_detail() -> String {
    "any compatible server · you set the URL".to_owned()
}

fn api_detail(api: ApiProtocol) -> String {
    format!("{} · you set the URL", api.display_name())
}

fn url_host(url: &str) -> &str {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_, remainder)| remainder);
    without_scheme.split('/').next().unwrap_or(without_scheme)
}

fn environment_badge(name: &str) -> Badge {
    if std::env::var_os(name).is_some_and(|value| !value.is_empty()) {
        Badge::new(format!("{name} is set"), Tone::Success)
    } else {
        Badge::new(format!("{name} is not set"), Tone::Warning)
    }
}

fn separator() -> String {
    presentation::paint(" · ", Tone::Muted, false)
}

fn saved_message(id: &str, profile: &AssistantProfile) -> String {
    format!(
        "{} {}{}{}{}{}{}{}",
        presentation::paint("Saved profile", Tone::Success, false),
        presentation::paint_emphasis(id, Tone::Accent),
        separator(),
        presentation::terminal_text(profile.provider.display_name()),
        separator(),
        presentation::terminal_text(profile.api.short_name()),
        separator(),
        presentation::terminal_text(&profile.model),
    )
}

fn default_message(config: &AssistantConfig, id: &str) -> String {
    match config.default_profile_id() {
        Some(default) if default == id => format!(
            "  {}",
            presentation::paint("now the default profile", Tone::Secondary, false)
        ),
        Some(default) => format!(
            "  {} {}{}{} {}",
            presentation::paint("default profile:", Tone::Secondary, false),
            presentation::terminal_text(default),
            separator(),
            presentation::paint("switch with", Tone::Secondary, false),
            presentation::paint(format!("inifra assistant use {id}"), Tone::Accent, false),
        ),
        None => String::new(),
    }
}

fn restore_config(path: &Path, original: Option<AssistantConfig>) -> Result<(), Box<dyn Error>> {
    match original {
        Some(config) => config.save(path)?,
        None => match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        },
    }

    Ok(())
}

/// The base URL field, asked only when the connection needs one or the profile already has one.
fn endpoint_fields(
    template: ConnectionTemplate,
    existing: Option<&AssistantProfile>,
) -> Vec<TextField> {
    let existing = existing.filter(|profile| {
        profile.api == template.api
            && (profile.provider == template.provider
                || (template.provider == Provider::CustomEndpoint
                    && profile.provider == Provider::OpenAiCompatible
                    && template.api == ApiProtocol::OpenAiChatCompletions))
    });
    if !(template.base_url_required || existing.is_some_and(|profile| profile.base_url.is_some())) {
        return Vec::new();
    }
    let field = TextField::new("base_url", "API base URL")
        .hint("the API root, such as https://host/v1")
        .default_value(existing.and_then(|profile| profile.base_url.clone()));
    vec![if template.base_url_required {
        field.required()
    } else {
        field
    }]
}

fn effort_field(default: Option<String>) -> TextField {
    TextField::new("effort", "Default effort")
        .hint("low, medium, or high; leave empty for the provider default")
        .default_value(default)
        .check(|value| {
            Effort::parse(value)
                .map(|_| ())
                .ok_or_else(|| "Use low, medium, or high, or leave it empty.".to_owned())
        })
}

/// Whether setup should ask about an API key for this connection.
fn key_possible(template: ConnectionTemplate, existing: Option<&AssistantProfile>) -> bool {
    template.api_key_required
        || template.api_key_optional
        || existing.is_some_and(|profile| profile.api_key_env.is_some())
}

/// The variable that names the key: the saved one, the provider's usual one, or `<NAME>_API_KEY`.
fn key_variable_name(
    template: ConnectionTemplate,
    profile_id: &str,
    existing: Option<&AssistantProfile>,
) -> String {
    existing
        .and_then(|profile| profile.api_key_env.clone())
        .or_else(|| template.api_key_env.map(str::to_owned))
        .unwrap_or_else(|| format!("{}_API_KEY", profile_id.to_uppercase().replace('-', "_")))
}

fn is_variable_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// How the API key reaches Inifra.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyChoice {
    None,
    Paste,
    Variable,
    OtherVariable,
    Keep,
}

/// The key choices to offer, in order. A key is pasted by default; `None` appears only when the
/// provider can work without one.
fn key_options(
    template: ConnectionTemplate,
    name: &str,
    keep_stored: bool,
) -> Vec<(KeyChoice, Row)> {
    let mut options = Vec::new();
    if !template.api_key_required {
        options.push((
            KeyChoice::None,
            Row::new("No API key").detail("for servers that need none"),
        ));
    }
    options.push((
        KeyChoice::Paste,
        Row::new("Paste an API key").detail("stored in credentials.env, not assistant.toml"),
    ));
    if keep_stored {
        options.push((
            KeyChoice::Keep,
            Row::new("Keep the stored key").detail("already saved"),
        ));
    }
    options.push((
        KeyChoice::Variable,
        Row::new("Use an environment variable").badge(environment_badge(name)),
    ));
    options.push((
        KeyChoice::OtherVariable,
        Row::new("Use a different variable").detail("you choose the name"),
    ));
    options
}

fn existing_key_choice(
    existing: Option<&AssistantProfile>,
    keep_stored: bool,
) -> Option<KeyChoice> {
    let profile = existing?;
    match (&profile.api_key_env, profile.credential_source) {
        (None, _) => Some(KeyChoice::None),
        (Some(_), CredentialSource::Environment) => Some(KeyChoice::Variable),
        (Some(_), CredentialSource::CredentialsFile) if keep_stored => Some(KeyChoice::Keep),
        (Some(_), CredentialSource::CredentialsFile) => Some(KeyChoice::Paste),
    }
}

/// Remembered answers for the connection steps, so going back keeps the previous choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConnectionChoice {
    custom: bool,
    provider_index: usize,
    api_index: usize,
    /// Skip the known/custom question and reopen the list that was last shown.
    resume_at_list: bool,
}

enum Selected {
    Template(ConnectionTemplate),
    Back,
    Quit,
}

impl ConnectionChoice {
    fn initial(existing: Option<&AssistantProfile>, presets: &[ProviderPreset]) -> Self {
        let provider_index = existing.and_then(|profile| {
            presets
                .iter()
                .position(|preset| preset.provider == profile.provider)
        });
        Self {
            custom: existing.is_some() && provider_index.is_none(),
            provider_index: provider_index.unwrap_or_default(),
            api_index: existing
                .and_then(|profile| ApiProtocol::ALL.iter().position(|api| *api == profile.api))
                .unwrap_or_default(),
            resume_at_list: false,
        }
    }
}

/// Asks whether to use a known provider or a custom endpoint, then lists only what that needs.
fn select_connection(
    heading: &Heading,
    choice: &mut ConnectionChoice,
    presets: &[ProviderPreset],
    existing: Option<&AssistantProfile>,
) -> Result<Selected, Box<dyn Error>> {
    loop {
        if !choice.resume_at_list {
            let screen = SelectScreen {
                heading: heading.clone(),
                choices: vec![
                    Row::new("Known provider").detail(known_provider_detail(presets)),
                    Row::new("Custom endpoint").detail(custom_endpoint_detail()),
                ],
                initial: usize::from(choice.custom),
                ..SelectScreen::default()
            };
            match run_select(&screen)? {
                InteractionOutcome::Selected(selected) => choice.custom = selected == 1,
                InteractionOutcome::Back => return Ok(Selected::Back),
                InteractionOutcome::Quit => return Ok(Selected::Quit),
            }
        }
        choice.resume_at_list = false;

        let (rows, initial, context): (Vec<Row>, usize, &str) = if choice.custom {
            (
                ApiProtocol::ALL
                    .iter()
                    .map(|api| Row::new(api.short_name()).detail(api_detail(*api)))
                    .collect(),
                choice.api_index,
                "custom endpoint",
            )
        } else {
            (
                presets
                    .iter()
                    .map(|preset| {
                        Row::new(preset.provider.display_name()).detail(preset_detail(*preset))
                    })
                    .collect(),
                choice.provider_index,
                "provider",
            )
        };
        let screen = SelectScreen {
            heading: heading.clone().context(context),
            choices: rows,
            initial,
            ..SelectScreen::default()
        };
        match run_select(&screen)? {
            InteractionOutcome::Selected(selected) => {
                if choice.custom {
                    choice.api_index = selected;
                } else {
                    choice.provider_index = selected;
                }
                return Ok(Selected::Template(selected_connection(
                    *choice, presets, existing,
                )?));
            }
            InteractionOutcome::Back => {}
            InteractionOutcome::Quit => return Ok(Selected::Quit),
        }
    }
}

fn selected_connection(
    choice: ConnectionChoice,
    presets: &[ProviderPreset],
    existing: Option<&AssistantProfile>,
) -> Result<ConnectionTemplate, Box<dyn Error>> {
    let template = if choice.custom {
        ApiProtocol::ALL
            .get(choice.api_index)
            .and_then(|api| connection_template(Provider::CustomEndpoint, *api))
    } else {
        let preset = presets
            .get(choice.provider_index)
            .ok_or("selected provider preset is unavailable")?;
        let api = existing
            .filter(|profile| {
                profile.provider == preset.provider && preset.apis.contains(&profile.api)
            })
            .map(|profile| profile.api)
            .unwrap_or(preset.default_api);
        preset.connection(api)
    };

    template.ok_or_else(|| "selected connection is not supported".into())
}

fn required_value(values: &BTreeMap<String, String>, key: &str) -> Result<String, Box<dyn Error>> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing form value {key:?}").into())
}

#[cfg(test)]
mod tests {
    use super::{
        ConnectionChoice, Draft, KeyChoice, Setup, Step, api_detail, default_message, effort_field,
        endpoint_fields, existing_key_choice, is_variable_name, key_options, key_possible,
        key_variable_name, preset_detail, profile_id_field, saved_message, selected_connection,
        short_reason, url_host,
    };
    use inifra_assistant::{
        ApiProtocol, AssistantConfig, AssistantProfile, ConnectionTemplate, CredentialSource,
        PROVIDER_PRESETS, Provider, connection_template,
    };
    fn template(provider: Provider, api: ApiProtocol) -> ConnectionTemplate {
        connection_template(provider, api).unwrap()
    }

    #[test]
    fn custom_api_fields_require_an_endpoint_and_leave_the_key_to_its_own_step() {
        let fields = endpoint_fields(
            template(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions),
            None,
        );
        let base_url = fields
            .iter()
            .find(|field| field.name == "base_url")
            .unwrap();

        assert!(base_url.required);
        assert!(!fields.iter().any(|field| field.name == "api_key_env"));
    }

    #[test]
    fn key_step_depends_on_what_the_connection_needs() {
        let ollama = template(Provider::Ollama, ApiProtocol::OllamaChat);
        assert!(!key_possible(ollama, None));
        let remote = AssistantProfile {
            api_key_env: Some("REMOTE_API_KEY".to_owned()),
            ..profile(Provider::Ollama, ApiProtocol::OllamaChat, "llama3.2")
        };
        assert!(key_possible(ollama, Some(&remote)));
        assert!(key_possible(
            template(Provider::OpenAi, ApiProtocol::OpenAiResponses),
            None
        ));
        assert!(key_possible(
            template(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions),
            None
        ));
    }

    #[test]
    fn key_variable_name_prefers_saved_then_preset_then_the_profile_name() {
        let nvidia = template(Provider::NvidiaNim, ApiProtocol::OpenAiChatCompletions);
        assert_eq!(key_variable_name(nvidia, "work", None), "NVIDIA_API_KEY");
        let custom = template(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions);
        assert_eq!(
            key_variable_name(custom, "home-lab", None),
            "HOME_LAB_API_KEY"
        );
        let saved = AssistantProfile {
            api_key_env: Some("MY_KEY".to_owned()),
            ..profile(Provider::NvidiaNim, ApiProtocol::OpenAiChatCompletions, "m")
        };
        assert_eq!(key_variable_name(nvidia, "work", Some(&saved)), "MY_KEY");
        assert!(is_variable_name(&key_variable_name(
            custom, "home-lab", None
        )));
    }

    #[test]
    fn pasting_is_the_first_key_choice_and_no_key_is_offered_only_when_optional() {
        let openai = key_options(
            template(Provider::OpenAi, ApiProtocol::OpenAiResponses),
            "OPENAI_API_KEY",
            false,
        );
        assert_eq!(openai[0].0, KeyChoice::Paste);
        assert!(!openai.iter().any(|(kind, _)| *kind == KeyChoice::None));
        assert!(!openai.iter().any(|(kind, _)| *kind == KeyChoice::Keep));

        let custom = key_options(
            template(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions),
            "WORK_API_KEY",
            true,
        );
        let kinds = custom.iter().map(|(kind, _)| *kind).collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                KeyChoice::None,
                KeyChoice::Paste,
                KeyChoice::Keep,
                KeyChoice::Variable,
                KeyChoice::OtherVariable
            ]
        );
    }

    #[test]
    fn existing_profiles_reopen_on_their_current_key_choice() {
        let mut saved = profile(Provider::OpenAi, ApiProtocol::OpenAiResponses, "gpt-5");
        saved.api_key_env = Some("OPENAI_API_KEY".to_owned());
        assert_eq!(
            existing_key_choice(Some(&saved), false),
            Some(KeyChoice::Variable)
        );
        saved.credential_source = CredentialSource::CredentialsFile;
        assert_eq!(
            existing_key_choice(Some(&saved), true),
            Some(KeyChoice::Keep)
        );
        assert_eq!(
            existing_key_choice(Some(&saved), false),
            Some(KeyChoice::Paste)
        );
        saved.api_key_env = None;
        assert_eq!(
            existing_key_choice(Some(&saved), false),
            Some(KeyChoice::None)
        );
        assert_eq!(existing_key_choice(None, false), None);
    }

    #[test]
    fn built_in_provider_fields_use_preset_defaults_without_asking_for_a_base_url() {
        let fields = endpoint_fields(
            template(Provider::NvidiaNim, ApiProtocol::OpenAiChatCompletions),
            None,
        );
        assert!(!fields.iter().any(|field| field.name == "base_url"));
    }

    #[test]
    fn choice_details_stay_short_and_name_the_first_providers() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let known = super::known_provider_detail(&presets);
        assert!(known.starts_with(presets[0].provider.display_name()));
        assert!(known.ends_with(&format!("+{} more", presets.len() - 3)));
        assert!(known.len() <= 50);
        assert!(super::custom_endpoint_detail().len() <= 50);
        for preset in &presets {
            let detail = preset_detail(*preset);
            assert!(detail.len() <= 54, "{detail}");
        }
    }

    #[test]
    fn new_profiles_start_with_a_known_provider() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let choice = ConnectionChoice::initial(None, &presets);
        assert!(!choice.custom);
        assert_eq!(choice.provider_index, 0);
        assert_eq!(choice.api_index, 0);
    }

    #[test]
    fn provider_selection_uses_the_preset_default_protocol() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let template = selected_connection(
            ConnectionChoice {
                custom: false,
                provider_index: 0,
                api_index: 2,
                resume_at_list: false,
            },
            &presets,
            None,
        )
        .unwrap();

        assert_eq!(template.provider, Provider::OpenAi);
        assert_eq!(template.api, ApiProtocol::OpenAiResponses);
    }

    #[test]
    fn protocol_selection_uses_the_shared_custom_api_template() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let template = selected_connection(
            ConnectionChoice {
                custom: true,
                provider_index: 0,
                api_index: 2,
                resume_at_list: false,
            },
            &presets,
            None,
        )
        .unwrap();

        assert_eq!(template.provider, Provider::CustomEndpoint);
        assert_eq!(template.api, ApiProtocol::AnthropicMessages);
        assert!(template.base_url_required);
    }

    #[test]
    fn existing_provider_protocol_is_preserved_when_editing_that_provider() {
        let presets = PROVIDER_PRESETS
            .iter()
            .filter(|preset| preset.selectable)
            .copied()
            .collect::<Vec<_>>();
        let existing = AssistantProfile {
            provider: Provider::OpenAi,
            api: ApiProtocol::OpenAiChatCompletions,
            model: "test-model".to_owned(),
            base_url: None,
            api_key_env: Some("OPENAI_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        };
        let template = selected_connection(
            ConnectionChoice {
                custom: false,
                provider_index: 0,
                api_index: 0,
                resume_at_list: false,
            },
            &presets,
            Some(&existing),
        )
        .unwrap();

        assert_eq!(template.provider, Provider::OpenAi);
        assert_eq!(template.api, ApiProtocol::OpenAiChatCompletions);
    }

    #[test]
    fn editing_a_legacy_custom_profile_keeps_its_base_url() {
        let existing = AssistantProfile {
            provider: Provider::OpenAiCompatible,
            api: ApiProtocol::OpenAiChatCompletions,
            model: "test-model".to_owned(),
            base_url: Some("https://api.example.test/v1".to_owned()),
            api_key_env: Some("TEST_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        };
        let fields = endpoint_fields(
            template(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions),
            Some(&existing),
        );

        assert_eq!(
            fields
                .iter()
                .find(|field| field.name == "base_url")
                .unwrap()
                .default
                .as_deref(),
            Some("https://api.example.test/v1")
        );
    }

    fn preset(provider: Provider) -> inifra_assistant::ProviderPreset {
        *PROVIDER_PRESETS
            .iter()
            .find(|preset| preset.provider == provider)
            .unwrap()
    }

    fn profile(provider: Provider, api: ApiProtocol, model: &str) -> AssistantProfile {
        AssistantProfile {
            provider,
            api,
            model: model.to_owned(),
            base_url: None,
            api_key_env: (provider != Provider::Ollama).then(|| "TEST_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: None,
        }
    }

    #[test]
    fn provider_details_name_the_protocol_host_and_credential() {
        assert_eq!(
            preset_detail(preset(Provider::OpenAi)),
            "Responses · api.openai.com"
        );
        assert_eq!(
            preset_detail(preset(Provider::Ollama)),
            "Ollama Chat · localhost:11434 · no key"
        );
        assert!(api_detail(ApiProtocol::AnthropicMessages).starts_with("Messages API · "));
        assert_eq!(url_host("https://openrouter.ai/api/v1"), "openrouter.ai");
        assert_eq!(url_host("localhost:8080"), "localhost:8080");
    }

    #[test]
    fn profile_id_field_rejects_invalid_and_taken_ids() {
        let mut config = AssistantConfig::default();
        config
            .insert_profile(
                "work",
                profile(Provider::OpenAi, ApiProtocol::OpenAiResponses, "gpt-5"),
            )
            .unwrap();
        let field = profile_id_field(&config);
        let check = field.check.as_ref().unwrap();

        assert!(field.required);
        assert!(check("1st").is_err());
        assert!(check("home lab").is_err());
        assert!(check("work").unwrap_err().contains("already exists"));
        assert!(check("Work").unwrap_err().contains("already exists"));
        assert_eq!(check("Nvidia-Test"), Ok(()));
        assert_eq!(
            super::normalize_profile_name("  Nvidia-Test "),
            "nvidia-test"
        );
        assert_eq!(check("home-lab"), Ok(()));
    }

    fn setup<'a>(config: &'a AssistantConfig, profile_id: Option<&'a str>) -> Setup<'a> {
        Setup {
            choice: ConnectionChoice::initial(None, &[]),
            config,
            credentials_path: std::path::Path::new("credentials.env"),
            profile_id,
            existing: None,
            presets: Vec::new(),
            single_step: false,
        }
    }

    #[test]
    fn steps_follow_what_the_connection_needs() {
        let config = AssistantConfig::default();
        let setup = setup(&config, None);
        let mut draft = Draft::default();
        assert_eq!(
            setup.steps(&draft),
            [
                Step::Connection,
                Step::Name,
                Step::Key,
                Step::Model,
                Step::Effort
            ]
        );

        draft.template = Some(template(
            Provider::CustomEndpoint,
            ApiProtocol::OpenAiChatCompletions,
        ));
        assert_eq!(
            setup.steps(&draft),
            [
                Step::Connection,
                Step::Name,
                Step::BaseUrl,
                Step::Key,
                Step::Model,
                Step::Effort
            ]
        );

        draft.template = Some(template(Provider::Ollama, ApiProtocol::OllamaChat));
        assert_eq!(
            setup.steps(&draft),
            [Step::Connection, Step::Name, Step::Model, Step::Effort]
        );
        let editing = super::Setup {
            profile_id: Some("local"),
            ..setup
        };
        assert!(!editing.steps(&draft).contains(&Step::Name));
    }

    #[test]
    fn the_outline_marks_progress_and_hangs_a_branch_under_the_current_step() {
        use inifra_tui::StepState;
        let config = AssistantConfig::default();
        let setup = setup(&config, None);
        let draft = Draft {
            template: Some(template(Provider::OpenAi, ApiProtocol::OpenAiResponses)),
            name: Some("work".to_owned()),
            ..Draft::default()
        };
        let rows = setup.outline_rows(&draft, Step::Key, Some(("Paste a key", None)));
        let summary = rows
            .iter()
            .map(|row| (row.label.as_str(), row.state))
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                ("Connection", StepState::Done),
                ("Profile name", StepState::Done),
                ("API key", StepState::Active),
                ("Default model", StepState::Pending),
                ("Default effort", StepState::Pending),
            ]
        );
        assert_eq!(rows[0].value.as_deref(), Some("OpenAI · Responses"));
        assert_eq!(rows[1].value.as_deref(), Some("work"));
        assert_eq!(rows[2].branch, Some(("Paste a key".to_owned(), None)));
        assert!(rows[3].branch.is_none());
    }

    fn saved(provider: Provider, api: ApiProtocol) -> AssistantProfile {
        AssistantProfile {
            provider,
            api,
            model: "gpt-5".to_owned(),
            base_url: None,
            api_key_env: Some("OPENAI_API_KEY".to_owned()),
            credential_source: CredentialSource::Environment,
            effort: Some(inifra_assistant::Effort::Low),
        }
    }

    #[test]
    fn edit_lists_each_part_of_a_profile_with_its_current_value() {
        let profile = saved(Provider::OpenAi, ApiProtocol::OpenAiResponses);
        let parts = super::edit_parts(super::existing_template(&profile).unwrap(), &profile);
        let described = parts
            .iter()
            .map(|(part, row)| (*part, row.label.as_str(), row.detail.clone().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            described,
            [
                (super::Part::Model, "Model", "gpt-5".to_owned()),
                (
                    super::Part::Key,
                    "API key",
                    "OPENAI_API_KEY (environment)".to_owned()
                ),
                (super::Part::Effort, "Effort", "low".to_owned()),
                (
                    super::Part::Connection,
                    "Connection",
                    "OpenAI · Responses".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn edit_offers_the_base_url_only_when_a_profile_has_one_and_no_key_for_keyless_ones() {
        let mut custom = saved(Provider::CustomEndpoint, ApiProtocol::OpenAiChatCompletions);
        custom.base_url = Some("https://host.example/v1".to_owned());
        let parts = super::edit_parts(super::existing_template(&custom).unwrap(), &custom);
        assert!(parts.iter().any(|(part, _)| *part == super::Part::BaseUrl));

        let mut local = saved(Provider::Ollama, ApiProtocol::OllamaChat);
        local.api_key_env = None;
        local.effort = None;
        let parts = super::edit_parts(super::existing_template(&local).unwrap(), &local);
        assert!(!parts.iter().any(|(part, _)| *part == super::Part::Key));
        assert_eq!(
            parts
                .iter()
                .find(|(part, _)| *part == super::Part::Effort)
                .unwrap()
                .1
                .detail
                .as_deref(),
            Some("provider default")
        );
    }

    #[test]
    fn legacy_openai_compatible_profiles_edit_as_custom_endpoints() {
        let legacy = saved(
            Provider::OpenAiCompatible,
            ApiProtocol::OpenAiChatCompletions,
        );
        let template = super::existing_template(&legacy).unwrap();
        assert_eq!(template.provider, Provider::CustomEndpoint);
    }

    #[test]
    fn editing_one_part_keeps_every_other_answer() {
        let mut profile = saved(Provider::OpenAi, ApiProtocol::OpenAiResponses);
        profile.credential_source = CredentialSource::CredentialsFile;
        let template = super::existing_template(&profile).unwrap();
        let mut draft = Draft::from_profile(template, "work", &profile);
        draft.model = Some("gpt-5-mini".to_owned());

        let (updated, key) = super::build_profile(&mut draft).unwrap();
        assert_eq!(updated.model, "gpt-5-mini");
        assert_eq!(updated.effort, profile.effort);
        assert_eq!(updated.api_key_env, profile.api_key_env);
        assert_eq!(updated.credential_source, CredentialSource::CredentialsFile);
        // Nothing new was pasted, so the stored key is left alone.
        assert!(key.secret.is_none());
    }

    #[test]
    fn single_step_headings_name_the_part_without_an_outline() {
        let config = AssistantConfig::default();
        let profile = saved(Provider::OpenAi, ApiProtocol::OpenAiResponses);
        let template = super::existing_template(&profile).unwrap();
        let mut single = setup(&config, Some("work"));
        single.single_step = true;
        let draft = Draft::from_profile(template, "work", &profile);
        // The outline rows still describe every step; only the heading chooses not to show them.
        assert_eq!(single.outline_rows(&draft, Step::Model, None).len(), 5);
        let _ = single.heading(&draft, Step::Model, None);
    }

    #[test]
    fn effort_field_defaults_to_the_saved_effort_and_only_accepts_known_levels() {
        let field = effort_field(Some("high".to_owned()));
        assert_eq!(field.default.as_deref(), Some("high"));
        assert!(!field.required);
        let check = field.check.unwrap();
        assert_eq!(check("medium"), Ok(()));
        assert!(check("extreme").is_err());
    }

    #[test]
    fn model_lookup_failures_name_only_what_a_user_can_fix() {
        assert_eq!(
            short_reason("API key is missing; configure the profile credential"),
            "the model list needs an API key"
        );
        assert_eq!(
            short_reason("HTTP 401 Unauthorized"),
            "the provider rejected the API key"
        );
        assert_eq!(
            short_reason("error sending request"),
            "could not fetch the model list"
        );
    }

    #[test]
    fn save_messages_name_the_connection_and_the_default() {
        let mut config = AssistantConfig::default();
        let local = profile(Provider::Ollama, ApiProtocol::OllamaChat, "qwen3:8b");
        config.insert_profile("local", local.clone()).unwrap();
        config
            .insert_profile(
                "work",
                profile(Provider::OpenAi, ApiProtocol::OpenAiResponses, "gpt-5"),
            )
            .unwrap();

        assert_eq!(
            saved_message("local", &local),
            "Saved profile local · Ollama · Ollama Chat · qwen3:8b"
        );
        assert_eq!(
            default_message(&config, "local"),
            "  now the default profile"
        );
        assert_eq!(
            default_message(&config, "work"),
            "  default profile: local · switch with inifra assistant use work"
        );
    }
}
