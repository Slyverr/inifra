use crate::assistant::UiExit;
use crate::assistant::configure::{configure_profile, edit_profile, key_summary};
use clap::Subcommand;
use inifra_assistant::{AssistantConfig, AssistantProfile};
use inifra_tui::{
    Badge, Heading, InteractionOutcome, Row, SelectScreen, presentation, presentation::Tone,
    run_confirmation, run_select,
};
use std::{
    error::Error,
    fs,
    io::IsTerminal,
    path::{Path, PathBuf},
};

pub(crate) const CONFIG_FILE: &str = "assistant.toml";
pub(crate) const CREDENTIALS_FILE: &str = "credentials.env";

/// Non-interactive assistant profile actions and the interactive manager.
#[derive(Debug, Subcommand)]
#[command(styles = crate::cli_style::styles())]
pub(crate) enum AssistantCommand {
    /// Show configured profiles.
    Profiles,
    /// Choose the profile used by default.
    Use {
        /// Profile identifier.
        id: String,
    },
    /// Create or edit a profile in the inline configuration flow.
    Configure {
        /// Existing profile identifier to edit.
        id: Option<String>,
    },
    /// Remove a profile after confirmation.
    Remove {
        /// Profile identifier.
        id: String,
    },
    /// Remove assistant profiles and saved credentials after confirmation.
    Reset,
}

/// Runs assistant profile management without loading the template catalog.
pub(crate) fn run(
    command: Option<AssistantCommand>,
    config_dir: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let config_dir = config_dir.ok_or("could not determine the user configuration directory")?;
    let config_path = config_dir.join(CONFIG_FILE);
    let credentials_path = config_dir.join(CREDENTIALS_FILE);
    match command {
        None => manage(&config_path, &credentials_path),
        Some(AssistantCommand::Profiles) => list_profiles(&config_path),
        Some(AssistantCommand::Use { id }) => set_default(&config_path, &id),
        Some(AssistantCommand::Configure { id }) => {
            require_terminal()?;
            let config = load_config(&config_path)?;
            configure_profile(&config_path, &credentials_path, config, id.as_deref()).map(|_| ())
        }
        Some(AssistantCommand::Remove { id }) => {
            require_terminal()?;
            remove_profile(&config_path, &id).map(|_| ())
        }
        Some(AssistantCommand::Reset) => {
            require_terminal()?;
            reset(&config_path, &credentials_path).map(|_| ())
        }
    }
}

/// A row in the manager's main list: a saved profile or one of the global actions.
#[derive(Clone, Debug, Eq, PartialEq)]
enum MainChoice {
    Profile(String),
    Create,
    Reset,
}

/// What can be done to one saved profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfileAction {
    Edit,
    MakeDefault,
    Remove,
}

/// The main list: every saved profile first, then Create, then Reset when there is data to reset.
fn main_choices(config: &AssistantConfig, has_data: bool) -> Vec<(MainChoice, Row)> {
    let mut choices = config
        .profiles()
        .iter()
        .map(|(id, profile)| {
            let row = Row::new(id.clone()).detail(format!(
                "{} · {}",
                profile.provider.display_name(),
                profile.api.short_name()
            ));
            let row = if config.default_profile_id() == Some(id.as_str()) {
                row.badge(Badge::new("default", Tone::Success))
            } else {
                row
            };
            (MainChoice::Profile(id.clone()), row)
        })
        .collect::<Vec<_>>();
    choices.push((MainChoice::Create, Row::new("Create a profile")));
    if has_data {
        choices.push((MainChoice::Reset, Row::new("Reset assistant data")));
    }
    choices
}

fn manager_screen(config: &AssistantConfig, choices: &[(MainChoice, Row)]) -> SelectScreen {
    let mut screen = SelectScreen::new(
        Heading::new("assistant profiles"),
        choices.iter().map(|(_, row)| row.clone()),
    );
    if config.profiles().is_empty() {
        screen.note = Some("No profiles yet. Create one to use inifra ask.".to_owned());
    }
    screen
}

/// What a profile menu offers: edit always, make default unless it already is, remove always.
fn profile_actions(is_default: bool) -> Vec<(ProfileAction, &'static str)> {
    let mut actions = vec![(ProfileAction::Edit, "Edit")];
    if !is_default {
        actions.push((ProfileAction::MakeDefault, "Make default"));
    }
    actions.push((ProfileAction::Remove, "Remove"));
    actions
}

/// Read-only rows describing one profile, shown above its actions.
fn profile_summary(profile: &AssistantProfile) -> Vec<Row> {
    let mut rows = vec![
        Row::new("Connection").detail(format!(
            "{} · {}",
            profile.provider.display_name(),
            profile.api.short_name()
        )),
        Row::new("Model").detail(profile.model.clone()),
        Row::new("Effort").detail(
            profile
                .effort
                .map_or("provider default", |effort| effort.as_str())
                .to_owned(),
        ),
    ];
    if let Some(base_url) = &profile.base_url {
        rows.push(Row::new("API base URL").detail(base_url.clone()));
    }
    if profile.api_key_env.is_some() {
        rows.push(Row::new("API key").detail(key_summary(profile)));
    }
    rows
}

fn manage(config_path: &Path, credentials_path: &Path) -> Result<(), Box<dyn Error>> {
    require_terminal()?;
    let mut last: Option<MainChoice> = None;
    loop {
        let config = load_config(config_path)?;
        let choices = main_choices(&config, config_path.exists() || credentials_path.exists());
        let mut screen = manager_screen(&config, &choices);
        screen.initial = last
            .as_ref()
            .and_then(|last| choices.iter().position(|(choice, _)| choice == last))
            .or_else(|| {
                config.default_profile_id().and_then(|default| {
                    choices
                        .iter()
                        .position(|(choice, _)| *choice == MainChoice::Profile(default.to_owned()))
                })
            })
            .unwrap_or_default();

        let choice = match run_select(&screen)? {
            InteractionOutcome::Selected(selected) => choices[selected].0.clone(),
            InteractionOutcome::Back | InteractionOutcome::Quit => return Ok(()),
        };
        last = Some(choice.clone());

        let exit = match choice {
            MainChoice::Create => configure_profile(config_path, credentials_path, config, None)?,
            MainChoice::Profile(id) => profile_menu(config_path, credentials_path, &id)?,
            MainChoice::Reset => reset(config_path, credentials_path)?,
        };
        if exit == UiExit::Quit {
            return Ok(());
        }
    }
}

/// The actions for one profile. Editing and making default return here so the result is visible.
fn profile_menu(
    config_path: &Path,
    credentials_path: &Path,
    id: &str,
) -> Result<UiExit, Box<dyn Error>> {
    loop {
        let config = load_config(config_path)?;
        let Some(profile) = config.profile(id) else {
            return Ok(UiExit::Continue);
        };
        let is_default = config.default_profile_id() == Some(id);
        let actions = profile_actions(is_default);
        let mut heading = Heading::new("assistant profiles").context(id);
        if is_default {
            heading = heading.context("default");
        }
        let screen = SelectScreen {
            heading,
            summary: profile_summary(profile),
            choices: actions.iter().map(|(_, label)| Row::new(*label)).collect(),
            ..SelectScreen::default()
        };
        let action = match run_select(&screen)? {
            InteractionOutcome::Selected(selected) => actions[selected].0,
            InteractionOutcome::Back => return Ok(UiExit::Continue),
            InteractionOutcome::Quit => return Ok(UiExit::Quit),
        };
        match action {
            ProfileAction::Edit => {
                if edit_profile(config_path, credentials_path, id)? == UiExit::Quit {
                    return Ok(UiExit::Quit);
                }
            }
            ProfileAction::MakeDefault => set_default(config_path, id)?,
            ProfileAction::Remove => {
                let exit = remove_profile(config_path, id)?;
                if exit == UiExit::Quit || load_config(config_path)?.profile(id).is_none() {
                    return Ok(exit);
                }
            }
        }
    }
}

fn list_profiles(config_path: &Path) -> Result<(), Box<dyn Error>> {
    let config = load_config(config_path)?;
    if config.profiles().is_empty() {
        println!(
            "{} Run {} to create one.",
            presentation::terminal_text("No assistant profiles configured."),
            presentation::paint_emphasis("inifra assistant", presentation::Tone::Accent)
        );
        return Ok(());
    }

    for (id, profile) in config.profiles() {
        let marker = if config.default_profile_id() == Some(id.as_str()) {
            format!(
                "  {}",
                presentation::paint("default", presentation::Tone::Success, false)
            )
        } else {
            String::new()
        };
        println!(
            "{}{}  {}  {}  {}",
            presentation::paint_emphasis(id, presentation::Tone::Accent),
            marker,
            presentation::terminal_text(profile.provider.display_name()),
            presentation::terminal_text(profile.api.display_name()),
            presentation::terminal_text(&profile.model)
        );
    }

    Ok(())
}

fn set_default(config_path: &Path, id: &str) -> Result<(), Box<dyn Error>> {
    let mut config = load_config(config_path)?;
    config.set_default_profile(id)?;
    config.save(config_path)?;
    println!(
        "{} {}",
        presentation::paint("Default profile", Tone::Success, false),
        presentation::paint_emphasis(id, Tone::Accent)
    );
    Ok(())
}

fn remove_profile(config_path: &Path, id: &str) -> Result<UiExit, Box<dyn Error>> {
    let mut config = load_config(config_path)?;
    let Some(profile) = config.profile(id) else {
        return Err(format!("unknown assistant profile {id:?}").into());
    };

    let description = format!(
        "Remove {id} ({} · {})? Saved credentials are kept.",
        profile.provider.display_name(),
        profile.model
    );
    match run_confirmation(
        &Heading::new("remove profile").context(id),
        &description,
        false,
    )? {
        InteractionOutcome::Selected(true) => {}
        InteractionOutcome::Selected(false) | InteractionOutcome::Back => {
            return Ok(UiExit::Continue);
        }
        InteractionOutcome::Quit => return Ok(UiExit::Quit),
    }

    {
        config.remove_profile(id)?;
        config.save(config_path)?;
        println!(
            "{} {}",
            presentation::paint("Removed profile", Tone::Success, false),
            presentation::paint_emphasis(id, presentation::Tone::Accent)
        );
    }

    Ok(UiExit::Continue)
}

fn reset(config_path: &Path, credentials_path: &Path) -> Result<UiExit, Box<dyn Error>> {
    match run_confirmation(
        &Heading::new("reset assistant data"),
        "Remove all profiles and saved credentials?",
        false,
    )? {
        InteractionOutcome::Selected(true) => {}
        InteractionOutcome::Selected(false) | InteractionOutcome::Back => {
            return Ok(UiExit::Continue);
        }
        InteractionOutcome::Quit => return Ok(UiExit::Quit),
    }
    ensure_removable_file(config_path)?;
    ensure_removable_file(credentials_path)?;
    remove_if_present(config_path)?;
    remove_if_present(credentials_path)?;
    println!(
        "{}",
        presentation::paint(
            "Assistant data removed.",
            presentation::Tone::Success,
            false
        )
    );
    Ok(UiExit::Continue)
}

fn ensure_removable_file(path: &Path) -> Result<(), Box<dyn Error>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(format!(
            "refusing to remove non-regular assistant data: {}",
            path.display()
        )
        .into()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_if_present(path: &Path) -> Result<(), Box<dyn Error>> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn load_config(path: &Path) -> Result<AssistantConfig, Box<dyn Error>> {
    Ok(AssistantConfig::load(path)?.unwrap_or_default())
}

fn require_terminal() -> Result<(), Box<dyn Error>> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err("assistant configuration requires an interactive terminal".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        MainChoice, ProfileAction, main_choices, manager_screen, profile_actions, profile_summary,
    };
    use inifra_assistant::{
        ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Effort, Provider,
    };

    fn profile(model: &str) -> AssistantProfile {
        AssistantProfile {
            provider: Provider::Ollama,
            api: ApiProtocol::OllamaChat,
            model: model.to_owned(),
            base_url: None,
            api_key_env: None,
            credential_source: CredentialSource::Environment,
            effort: None,
        }
    }

    #[test]
    fn manager_offers_only_create_until_profiles_exist() {
        let choices = main_choices(&AssistantConfig::default(), false);
        assert_eq!(
            choices.iter().map(|(choice, _)| choice).collect::<Vec<_>>(),
            [&MainChoice::Create]
        );
        let with_data = main_choices(&AssistantConfig::default(), true);
        assert_eq!(with_data.last().unwrap().0, MainChoice::Reset);
    }

    #[test]
    fn profiles_are_the_rows_and_the_default_badge_always_fits() {
        let mut config = AssistantConfig::default();
        config.insert_profile("local", profile("qwen3:8b")).unwrap();
        config
            .insert_profile(
                "work",
                profile("a-very-long-model-name-that-used-to-push-the-badge-away"),
            )
            .unwrap();
        let choices = main_choices(&config, true);
        let kinds = choices
            .iter()
            .map(|(choice, _)| choice.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                MainChoice::Profile("local".to_owned()),
                MainChoice::Profile("work".to_owned()),
                MainChoice::Create,
                MainChoice::Reset
            ]
        );
        let local = &choices[0].1;
        // The model is left to the profile menu, so no row can outgrow a narrow terminal.
        assert_eq!(local.detail.as_deref(), Some("Ollama · Ollama Chat"));
        assert_eq!(local.badge.as_ref().unwrap().text, "default");
        assert!(choices[1].1.badge.is_none());
        assert!(choices[1].1.detail.as_deref().unwrap().len() < 40);

        let screen = manager_screen(&config, &choices);
        assert!(screen.note.is_none());
        assert_eq!(screen.choices.len(), 4);
    }

    #[test]
    fn the_empty_state_explains_how_to_start() {
        let config = AssistantConfig::default();
        let screen = manager_screen(&config, &main_choices(&config, false));
        assert!(screen.note.as_deref().unwrap().contains("inifra ask"));
    }

    #[test]
    fn make_default_is_hidden_for_the_profile_that_already_is_the_default() {
        let labels = |is_default| {
            profile_actions(is_default)
                .into_iter()
                .map(|(action, _)| action)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(false),
            [
                ProfileAction::Edit,
                ProfileAction::MakeDefault,
                ProfileAction::Remove
            ]
        );
        assert_eq!(labels(true), [ProfileAction::Edit, ProfileAction::Remove]);
    }

    #[test]
    fn the_profile_summary_shows_the_model_the_row_leaves_out() {
        let mut saved = profile("qwen3:8b");
        saved.effort = Some(Effort::High);
        saved.api_key_env = Some("REMOTE_API_KEY".to_owned());
        let rows = profile_summary(&saved);
        let get = |label: &str| {
            rows.iter()
                .find(|row| row.label == label)
                .and_then(|row| row.detail.clone())
        };
        assert_eq!(get("Model").as_deref(), Some("qwen3:8b"));
        assert_eq!(get("Effort").as_deref(), Some("high"));
        assert_eq!(
            get("API key").as_deref(),
            Some("REMOTE_API_KEY (environment)")
        );
        assert_eq!(get("Connection").as_deref(), Some("Ollama · Ollama Chat"));
        assert!(get("API base URL").is_none());
    }
}
