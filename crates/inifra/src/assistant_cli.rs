use crate::assistant_configure::{configure_profile, profile_id_input};
use clap::Subcommand;
use inifra_assistant::AssistantConfig;
use inifra_tui::{presentation, run_confirmation, run_select};
use std::{
    error::Error,
    fs,
    io::IsTerminal,
    path::{Path, PathBuf},
};

pub(crate) const CONFIG_FILE: &str = "assistant.toml";
pub(crate) const CREDENTIALS_FILE: &str = "credentials.env";

/// Non-interactive assistant profile actions and the interactive manager.
#[derive(Subcommand)]
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
            configure_profile(&config_path, &credentials_path, config, id.as_deref())
        }
        Some(AssistantCommand::Remove { id }) => {
            require_terminal()?;
            remove_profile(&config_path, &id)
        }
        Some(AssistantCommand::Reset) => {
            require_terminal()?;
            reset(&config_path, &credentials_path)
        }
    }
}

fn manage(config_path: &Path, credentials_path: &Path) -> Result<(), Box<dyn Error>> {
    require_terminal()?;
    loop {
        let config = load_config(config_path)?;
        let mut actions = vec!["Create or edit a profile".to_owned()];
        if !config.profiles().is_empty() {
            actions.extend([
                "Change default profile".to_owned(),
                "Remove a profile".to_owned(),
            ]);
        }
        if config_path.exists() || credentials_path.exists() {
            actions.push("Reset assistant data".to_owned());
        }
        let Some(selected) = run_select("", &actions, 0)? else {
            return Ok(());
        };
        match actions[selected].as_str() {
            "Create or edit a profile" => {
                let Some(id) = profile_id_input()? else {
                    continue;
                };
                configure_profile(config_path, credentials_path, config, Some(&id))?;
            }
            "Change default profile" => {
                if let Some(id) = choose_profile(&config, "Default profile")? {
                    set_default(config_path, &id)?;
                }
            }
            "Remove a profile" => {
                if let Some(id) = choose_profile(&config, "Remove profile")? {
                    remove_profile(config_path, &id)?;
                }
            }
            "Reset assistant data" => reset(config_path, credentials_path)?,
            _ => unreachable!("assistant actions are defined above"),
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
        presentation::paint_emphasis("Default assistant profile:", presentation::Tone::Accent),
        presentation::paint_emphasis(id, presentation::Tone::Accent)
    );
    Ok(())
}

fn choose_profile(config: &AssistantConfig, title: &str) -> Result<Option<String>, Box<dyn Error>> {
    let profiles = config
        .profiles()
        .iter()
        .map(|(id, profile)| {
            let is_default = config.default_profile_id() == Some(id.as_str());
            let label = if is_default {
                format!(
                    "{id}  ·  {}  ·  {}  ·  default",
                    profile.provider.display_name(),
                    profile.model
                )
            } else {
                format!(
                    "{id}  ·  {}  ·  {}",
                    profile.provider.display_name(),
                    profile.model
                )
            };
            (id.clone(), label)
        })
        .collect::<Vec<_>>();
    if profiles.is_empty() {
        return Ok(None);
    }
    let choices = profiles
        .iter()
        .map(|(_, label)| label.clone())
        .collect::<Vec<_>>();
    let selected = run_select(title, &choices, 0)?;
    Ok(selected.map(|index| profiles[index].0.clone()))
}

fn remove_profile(config_path: &Path, id: &str) -> Result<(), Box<dyn Error>> {
    let mut config = load_config(config_path)?;
    let Some(profile) = config.profile(id) else {
        return Err(format!("unknown assistant profile {id:?}").into());
    };
    let description = format!(
        "Remove profile {id:?} ({}, {})?",
        profile.provider.display_name(),
        profile.model
    );
    if run_confirmation("", &description, false)?.unwrap_or(false) {
        config.remove_profile(id)?;
        config.save(config_path)?;
        println!(
            "{} {}",
            presentation::paint(
                "Removed assistant profile",
                presentation::Tone::Success,
                false
            ),
            presentation::paint_emphasis(id, presentation::Tone::Accent)
        );
    }
    Ok(())
}

fn reset(config_path: &Path, credentials_path: &Path) -> Result<(), Box<dyn Error>> {
    if !run_confirmation("", "Remove all profiles and saved credentials?", false)?.unwrap_or(false)
    {
        return Ok(());
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
    Ok(())
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
