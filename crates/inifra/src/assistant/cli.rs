use crate::assistant::UiExit;
use crate::assistant::configure::configure_profile;
use clap::Subcommand;
use inifra_assistant::AssistantConfig;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagerAction {
    Create,
    Edit,
    ChangeDefault,
    Remove,
    Reset,
}

impl ManagerAction {
    fn label(self) -> &'static str {
        match self {
            Self::Create => "Create a profile",
            Self::Edit => "Edit a profile",
            Self::ChangeDefault => "Change default profile",
            Self::Remove => "Remove a profile",
            Self::Reset => "Reset assistant data",
        }
    }
}

fn manager_actions(has_profiles: bool, has_data: bool) -> Vec<ManagerAction> {
    let mut actions = vec![ManagerAction::Create];
    if has_profiles {
        actions.extend([
            ManagerAction::Edit,
            ManagerAction::ChangeDefault,
            ManagerAction::Remove,
        ]);
    }
    if has_data {
        actions.push(ManagerAction::Reset);
    }
    actions
}

/// Summary rows for saved profiles: ID, connection, model, and the default tag.
pub(crate) fn profile_rows(config: &AssistantConfig) -> Vec<Row> {
    config
        .profiles()
        .iter()
        .map(|(id, profile)| {
            let row = Row::new(id.clone()).detail(format!(
                "{} · {} · {}",
                profile.provider.display_name(),
                profile.api.short_name(),
                profile.model
            ));
            if config.default_profile_id() == Some(id.as_str()) {
                row.badge(Badge::new("default", Tone::Success))
            } else {
                row
            }
        })
        .collect()
}

fn manager_screen(config: &AssistantConfig, actions: &[ManagerAction]) -> SelectScreen {
    let mut screen = SelectScreen::new(
        Heading::new("assistant profiles"),
        actions.iter().map(|action| action.label()),
    );
    screen.summary = profile_rows(config);
    if screen.summary.is_empty() {
        screen.note = Some("No profiles yet. Create one to use inifra ask.".to_owned());
    }
    screen
}

fn manage(config_path: &Path, credentials_path: &Path) -> Result<(), Box<dyn Error>> {
    require_terminal()?;
    let mut last_action = ManagerAction::Create;
    loop {
        let config = load_config(config_path)?;
        let actions = manager_actions(
            !config.profiles().is_empty(),
            config_path.exists() || credentials_path.exists(),
        );
        let mut screen = manager_screen(&config, &actions);
        screen.initial = actions
            .iter()
            .position(|action| *action == last_action)
            .unwrap_or_default();

        let action = match run_select(&screen)? {
            InteractionOutcome::Selected(selected) => actions[selected],
            InteractionOutcome::Back | InteractionOutcome::Quit => return Ok(()),
        };
        last_action = action;

        let exit = match action {
            ManagerAction::Create => {
                configure_profile(config_path, credentials_path, config, None)?
            }
            ManagerAction::Edit => match choose_profile(&config, "edit profile")? {
                InteractionOutcome::Selected(id) => {
                    configure_profile(config_path, credentials_path, config, Some(&id))?
                }
                InteractionOutcome::Back => UiExit::Continue,
                InteractionOutcome::Quit => UiExit::Quit,
            },
            ManagerAction::ChangeDefault => match choose_profile(&config, "default profile")? {
                InteractionOutcome::Selected(id) => {
                    set_default(config_path, &id)?;
                    UiExit::Continue
                }
                InteractionOutcome::Back => UiExit::Continue,
                InteractionOutcome::Quit => UiExit::Quit,
            },
            ManagerAction::Remove => match choose_profile(&config, "remove profile")? {
                InteractionOutcome::Selected(id) => remove_profile(config_path, &id)?,
                InteractionOutcome::Back => UiExit::Continue,
                InteractionOutcome::Quit => UiExit::Quit,
            },
            ManagerAction::Reset => reset(config_path, credentials_path)?,
        };
        if exit == UiExit::Quit {
            return Ok(());
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

fn choose_profile(
    config: &AssistantConfig,
    task: &str,
) -> Result<InteractionOutcome<String>, Box<dyn Error>> {
    let ids = config.profiles().keys().cloned().collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(InteractionOutcome::Back);
    }

    let screen = SelectScreen {
        heading: Heading::new(task),
        choices: profile_rows(config),
        initial: config
            .default_profile_id()
            .and_then(|default| ids.iter().position(|id| id == default))
            .unwrap_or_default(),
        ..SelectScreen::default()
    };
    Ok(match run_select(&screen)? {
        InteractionOutcome::Selected(index) => InteractionOutcome::Selected(ids[index].clone()),
        InteractionOutcome::Back => InteractionOutcome::Back,
        InteractionOutcome::Quit => InteractionOutcome::Quit,
    })
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
    use super::{ManagerAction, manager_actions, manager_screen};
    use inifra_assistant::{
        ApiProtocol, AssistantConfig, AssistantProfile, CredentialSource, Provider,
    };

    #[test]
    fn manager_offers_only_create_until_profiles_exist() {
        assert_eq!(manager_actions(false, false), [ManagerAction::Create]);
        assert_eq!(
            manager_actions(true, true),
            [
                ManagerAction::Create,
                ManagerAction::Edit,
                ManagerAction::ChangeDefault,
                ManagerAction::Remove,
                ManagerAction::Reset,
            ]
        );
    }

    #[test]
    fn manager_summarizes_profiles_or_explains_the_empty_state() {
        let empty = manager_screen(&AssistantConfig::default(), &[ManagerAction::Create]);
        assert!(empty.summary.is_empty());
        assert!(empty.note.as_deref().unwrap().contains("inifra ask"));

        let mut config = AssistantConfig::default();
        config
            .insert_profile(
                "local",
                AssistantProfile {
                    provider: Provider::Ollama,
                    api: ApiProtocol::OllamaChat,
                    model: "qwen3:8b".to_owned(),
                    base_url: None,
                    api_key_env: None,
                    credential_source: CredentialSource::Environment,
                    effort: None,
                },
            )
            .unwrap();
        let screen = manager_screen(&config, &[ManagerAction::Create]);
        assert_eq!(screen.summary[0].label, "local");
        assert_eq!(
            screen.summary[0].detail.as_deref(),
            Some("Ollama · Ollama Chat · qwen3:8b")
        );
        assert_eq!(screen.summary[0].badge.as_ref().unwrap().text, "default");
        assert!(screen.note.is_none());
    }
}
