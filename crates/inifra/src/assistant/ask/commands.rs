use inifra_assistant::AssistantProfile;
use inifra_core::{catalog::Catalog, plan::GenerationPlan, workflow};
use std::{collections::BTreeMap, path::Path};

const COMMANDS: &[&str] = &[
    "/help",
    "/templates",
    "/inspect",
    "/project",
    "/profile",
    "/model",
    "/models",
    "/plan",
    "/apply",
];

pub(super) enum Command {
    Help,
    Templates(String),
    Inspect(String),
    Project,
    Profiles,
    Profile(String),
    Models,
    Model(String),
    Plan {
        ids: Vec<String>,
        values: BTreeMap<String, String>,
        overwrite: bool,
    },
    Apply,
}

pub(super) fn parse(input: &str) -> Result<Option<Command>, String> {
    if !input.trim_start().starts_with('/') {
        return Ok(None);
    }

    let tokens = shlex::split(input).ok_or_else(|| "unclosed quote in command".to_owned())?;
    let Some(command) = tokens.first().map(String::as_str) else {
        return Ok(None);
    };

    let arguments = &tokens[1..];
    let unexpected = || format!("unexpected argument for {command}; use /help");
    let parsed = match command {
        "/help" if arguments.is_empty() => Command::Help,
        "/templates" => Command::Templates(arguments.join(" ")),
        "/inspect" if arguments.len() == 1 => Command::Inspect(arguments[0].clone()),
        "/project" if arguments.is_empty() => Command::Project,
        "/profile" if arguments.is_empty() => Command::Profiles,
        "/profile" if arguments.len() == 1 => Command::Profile(arguments[0].clone()),
        "/models" if arguments.is_empty() => Command::Models,
        "/model" if arguments.len() == 1 => Command::Model(arguments[0].clone()),
        "/model" if arguments.is_empty() => {
            return Err("usage: /model <model-id> · use /models to list available IDs".to_owned());
        }
        "/plan" => parse_plan(arguments)?,
        "/apply" if arguments.is_empty() => Command::Apply,
        "/help" | "/inspect" | "/project" | "/profile" | "/models" | "/apply" => {
            return Err(unexpected());
        }
        "/model" => return Err(unexpected()),
        _ => return Err(format!("unknown command {command:?}; use /help")),
    };

    Ok(Some(parsed))
}

fn parse_plan(arguments: &[String]) -> Result<Command, String> {
    let mut ids = Vec::new();
    let mut values = BTreeMap::new();
    let mut overwrite = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--set" => {
                index += 1;
                let assignment = arguments
                    .get(index)
                    .ok_or_else(|| "/plan --set requires NAME=VALUE".to_owned())?;
                let (name, value) = assignment
                    .split_once('=')
                    .ok_or_else(|| "/plan --set expects NAME=VALUE".to_owned())?;
                if name.is_empty() || values.insert(name.to_owned(), value.to_owned()).is_some() {
                    return Err(format!("invalid or duplicate input assignment {name:?}"));
                }
            }
            "--overwrite" => overwrite = true,
            argument if argument.starts_with('-') => {
                return Err(format!("unknown /plan option {argument:?}"));
            }
            id => ids.push(id.to_owned()),
        }
        index += 1;
    }

    if ids.is_empty() {
        return Err("usage: /plan <template-id>... [--set NAME=VALUE] [--overwrite]".to_owned());
    }

    Ok(Command::Plan {
        ids,
        values,
        overwrite,
    })
}

pub(super) fn completions(
    input: &str,
    catalog: &Catalog,
    models: &[String],
    profiles: &BTreeMap<String, AssistantProfile>,
) -> Vec<String> {
    let prefix = input.trim_start();
    if !prefix.starts_with('/') {
        return Vec::new();
    }

    let Some((command, argument)) = prefix.split_once(char::is_whitespace) else {
        return COMMANDS
            .iter()
            .filter(|command| command.starts_with(prefix))
            .map(|command| (*command).to_owned())
            .collect();
    };

    let (partial, earlier) = argument
        .rsplit_once(char::is_whitespace)
        .map_or((argument.trim(), ""), |(earlier, partial)| {
            (partial.trim(), earlier)
        });
    let values = match command {
        "/model" => models.to_vec(),
        "/profile" => profiles.keys().cloned().collect(),
        "/inspect" | "/plan" => catalog.packs.iter().map(|pack| pack.id.clone()).collect(),
        _ => return Vec::new(),
    };
    values
        .iter()
        .filter(|value| value.starts_with(partial))
        .map(|value| {
            if command == "/plan" && !earlier.is_empty() {
                format!("{command} {earlier} {value}")
            } else {
                format!("{command} {value}")
            }
        })
        .collect()
}

pub(super) fn help() -> String {
    [
        "Chat with the configured assistant, or use local commands:",
        "",
        "/templates [query]                 Search this project's template catalog",
        "/inspect <id>                      Show one template's outputs and inputs",
        "/project                           Show known project files",
        "/profile [id]                     List or switch saved assistant profiles",
        "/models                            Fetch model IDs supported by the provider",
        "/model <id>                       Switch models for this session",
        "/plan <id>... [--set name=value]  Prepare a generation plan; no files are written",
        "/apply                             Review and explicitly approve the pending plan",
        "/help                              Show this help",
        "",
        "Local commands stay on this machine until you send a message; then their visible results may be included as context for the configured provider.",
    ]
    .join("\n")
}

pub(super) fn list_profiles(
    profiles: &BTreeMap<String, AssistantProfile>,
    active_id: &str,
) -> String {
    let width = profiles
        .keys()
        .map(|id| id.chars().count())
        .max()
        .unwrap_or_default();
    let mut lines = vec!["### Assistant profiles".to_owned()];
    for (id, profile) in profiles {
        let padding = " ".repeat(width - id.chars().count() + 3);
        let (label, tag) = if id == active_id {
            (format!("**{id}**"), "   *active*")
        } else {
            (id.clone(), "")
        };
        lines.push(format!(
            "- {label}{padding}{} · {} · {}{tag}",
            profile.provider.display_name(),
            profile.api.short_name(),
            profile.model
        ));
    }
    lines.push(
        "Switch for this session with `/profile ID`; the saved default stays the same.".to_owned(),
    );
    lines.join("\n")
}

pub(super) fn list_templates(catalog: &Catalog, query: &str) -> String {
    let query = query.to_lowercase();
    let mut lines = vec!["Templates".to_owned()];
    let mut count = 0;
    for pack in &catalog.packs {
        let category = catalog
            .categories
            .get(&pack.category)
            .map_or(pack.category.as_str(), |category| category.name.as_str());
        if !query.is_empty()
            && !pack.id.to_lowercase().contains(&query)
            && !pack.description.to_lowercase().contains(&query)
            && !category.to_lowercase().contains(&query)
        {
            continue;
        }

        if count == 30 {
            lines.push("… more matches; refine the query".to_owned());
            break;
        }
        lines.push(format!("{} · {} · {}", pack.id, category, pack.description));
        count += 1;
    }

    if count == 0 {
        lines.push("No matching templates.".to_owned());
    }
    lines.join("\n")
}

pub(super) fn inspect_template(catalog: &Catalog, id: &str) -> Result<String, String> {
    let pack = catalog
        .packs
        .iter()
        .find(|pack| pack.id == id)
        .ok_or_else(|| format!("unknown template {id:?}"))?;
    let category = catalog
        .categories
        .get(&pack.category)
        .map_or(pack.category.as_str(), |category| category.name.as_str());
    let outputs = catalog
        .resolve_outputs(&[id.to_owned()])
        .map_err(|error| error.to_string())?;
    let inputs = catalog
        .input_definitions_for(pack)
        .map_err(|error| error.to_string())?;
    let mut lines = vec![
        format!("{} · {}", pack.id, category),
        pack.description.clone(),
    ];
    if !outputs.is_empty() {
        lines.push("".to_owned());
        lines.push("Files".to_owned());
        lines.extend(
            outputs
                .iter()
                .map(|output| format!("{}", output.output.display())),
        );
    }

    if !pack.directories.is_empty() {
        lines.push("".to_owned());
        lines.push("Directories".to_owned());
        lines.extend(
            pack.directories
                .iter()
                .map(|path| format!("{}/", path.display())),
        );
    }

    if !inputs.is_empty() {
        lines.push("".to_owned());
        lines.push("Inputs".to_owned());
        lines.extend(inputs.iter().map(|(name, definition)| {
            let required = if definition.required {
                "required"
            } else {
                "optional"
            };
            format!("{name} ({required}) · {}", definition.prompt)
        }));
    }

    Ok(lines.join("\n"))
}

pub(super) fn inspect_project(root: &Path) -> String {
    const KNOWN_FILES: &[&str] = &[
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "composer.json",
        "Gemfile",
        "README.md",
        "AGENTS.md",
        "CONTRIBUTING.md",
        "CHANGELOG.md",
        "SECURITY.md",
    ];
    let present = KNOWN_FILES
        .iter()
        .filter(|name| root.join(name).is_file())
        .copied()
        .collect::<Vec<_>>();
    let manifests = present
        .iter()
        .copied()
        .filter(|name| {
            matches!(
                *name,
                "Cargo.toml"
                    | "package.json"
                    | "pyproject.toml"
                    | "go.mod"
                    | "composer.json"
                    | "Gemfile"
            )
        })
        .collect::<Vec<_>>();
    let guidance = present
        .iter()
        .copied()
        .filter(|name| !manifests.contains(name))
        .collect::<Vec<_>>();
    let mut lines = vec!["Known files in the current project directory".to_owned()];
    lines.push(format!(
        "Recognized manifests: {}",
        if manifests.is_empty() {
            "none".to_owned()
        } else {
            manifests.join(", ")
        }
    ));
    lines.push(format!(
        "Guidance files: {}",
        if guidance.is_empty() {
            "none".to_owned()
        } else {
            guidance.join(", ")
        }
    ));
    lines.join("\n")
}

pub(super) fn prepare_plan(
    catalog: &Catalog,
    root: &Path,
    ids: &[String],
    values: &BTreeMap<String, String>,
    overwrite: bool,
) -> Result<(String, GenerationPlan), String> {
    let definitions = workflow::input_definitions_for_selection(catalog, ids)
        .map_err(|error| error.to_string())?;
    let missing = definitions
        .iter()
        .filter(|(name, definition)| {
            definition.required
                && values
                    .get(*name)
                    .or(definition.default.as_ref())
                    .is_none_or(|value| value.trim().is_empty())
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        let mut lines = vec!["Required inputs are missing:".to_owned()];
        lines.extend(
            missing
                .iter()
                .map(|(name, definition)| format!("{name} · {}", definition.prompt)),
        );
        lines.push(format!(
            "Retry with {}",
            missing
                .iter()
                .map(|(name, _)| format!("--set {name}=\"value\""))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        return Err(lines.join("\n"));
    }

    let plan = workflow::prepare(catalog, ids, values, root, overwrite)
        .map_err(|error| error.to_string())?;
    let mut lines = vec!["Plan for the current project".to_owned()];
    for directory in &plan.directories {
        let action = match directory.action {
            inifra_core::plan::DirectoryAction::Create => "create directory",
            inifra_core::plan::DirectoryAction::Existing => "already exists",
        };
        lines.push(format!("{action} · {}/", directory.path.display()));
    }

    for file in &plan.files {
        let action = match file.action {
            inifra_core::plan::Action::Create => "create",
            inifra_core::plan::Action::Replace => "replace",
            inifra_core::plan::Action::Skip => "already exists",
        };
        lines.push(format!("{action} · {}", file.path.display()));
    }

    if plan.files.is_empty() && plan.directories.is_empty() {
        lines.push("The plan has no filesystem changes.".to_owned());
    } else {
        lines.push("No files have been changed. Use /apply to review and approve.".to_owned());
    }

    Ok((lines.join("\n"), plan))
}

pub(super) fn plan_confirmation(plan: &GenerationPlan) -> String {
    let files = plan.files.len();
    let directories = plan.directories.len();
    format!(
        "Apply this validated plan? {files} file operation(s), {directories} directory operation(s)."
    )
}

pub(super) fn plan_result(plan: &GenerationPlan) -> String {
    let created_files = plan
        .files
        .iter()
        .filter(|file| {
            matches!(
                file.action,
                inifra_core::plan::Action::Create | inifra_core::plan::Action::Replace
            )
        })
        .count();
    let created_directories = plan
        .directories
        .iter()
        .filter(|directory| directory.action == inifra_core::plan::DirectoryAction::Create)
        .count();
    format!("Applied the plan: {created_files} file(s), {created_directories} directory(ies).")
}

pub(super) fn completion_values(
    catalog: &Catalog,
    input: &str,
    models: &[String],
    profiles: &BTreeMap<String, AssistantProfile>,
) -> Vec<String> {
    let values = completions(input, catalog, models, profiles);
    values.into_iter().take(100).collect()
}

pub(super) fn requests_model_completion(input: &str, models: &[String]) -> bool {
    models.is_empty()
        && input
            .trim_start()
            .strip_prefix("/model")
            .is_some_and(|remainder| remainder.starts_with(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use super::{
        Command, completions, list_profiles, parse, prepare_plan, requests_model_completion,
    };
    use inifra_assistant::{ApiProtocol, AssistantProfile, CredentialSource, Provider};
    use inifra_core::catalog::Catalog;
    use std::{collections::BTreeMap, path::Path};

    #[test]
    fn parses_local_commands_and_plan_assignments() {
        assert!(matches!(parse("/help").unwrap(), Some(Command::Help)));
        assert!(matches!(
            parse("/profile").unwrap(),
            Some(Command::Profiles)
        ));
        assert!(matches!(
            parse("/profile nvidia").unwrap(),
            Some(Command::Profile(id)) if id == "nvidia"
        ));
        assert!(matches!(
            parse("/plan readme agents --set project_name=\"My Project\" --overwrite").unwrap(),
            Some(Command::Plan { ids, values, overwrite: true })
                if ids == ["readme", "agents"] && values.get("project_name").is_some_and(|v| v == "My Project")
        ));
    }

    #[test]
    fn rejects_duplicate_or_malformed_plan_values() {
        assert!(parse("/plan readme --set title=a --set title=b").is_err());
        assert!(parse("/plan readme --set title").is_err());
        assert!(parse("/plan readme --unknown").is_err());
    }

    #[test]
    fn non_commands_remain_regular_chat_messages() {
        assert!(parse("Help me choose a template").unwrap().is_none());
    }

    #[test]
    fn first_model_tab_requests_ids_then_uses_the_cached_list() {
        assert!(requests_model_completion("/model ", &[]));
        assert!(requests_model_completion("  /model gpt", &[]));
        assert!(!requests_model_completion("/models ", &[]));
        assert!(!requests_model_completion(
            "/model ",
            &["model-a".to_owned()]
        ));
    }

    #[test]
    fn profile_completion_lists_only_configured_profiles_without_credentials() {
        let catalog_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates");
        let catalog = Catalog::load(&catalog_root).unwrap();
        let profiles = BTreeMap::from([(
            "nvidia".to_owned(),
            AssistantProfile {
                provider: Provider::NvidiaNim,
                api: ApiProtocol::OpenAiChatCompletions,
                model: "nemotron-test".to_owned(),
                base_url: None,
                api_key_env: Some("NVIDIA_API_KEY".to_owned()),
                credential_source: CredentialSource::Environment,
            },
        )]);

        assert_eq!(
            completions("/profile n", &catalog, &[], &profiles),
            ["/profile nvidia"]
        );
        let listed = list_profiles(&profiles, "nvidia");
        assert!(
            listed.contains(
                "- **nvidia**   NVIDIA NIM · Chat Completions · nemotron-test   *active*"
            )
        );
        assert!(listed.contains("`/profile ID`"));
        assert!(!listed.contains("NVIDIA_API_KEY"));
    }

    #[test]
    fn preparing_a_plan_does_not_create_the_target() {
        let catalog_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates");
        let catalog = Catalog::load(&catalog_root).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("project");
        let (summary, plan) = prepare_plan(
            &catalog,
            &target,
            &["editorconfig".to_owned()],
            &BTreeMap::new(),
            false,
        )
        .unwrap();

        assert!(!target.exists());
        assert!(!plan.files.is_empty());
        assert!(summary.contains("No files have been changed"));
        assert!(summary.contains("current project"));
        assert!(!summary.contains(&target.display().to_string()));
    }

    #[test]
    fn missing_plan_inputs_are_reported_before_planning_writes() {
        let catalog_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates");
        let catalog = Catalog::load(&catalog_root).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("project");
        let error = prepare_plan(
            &catalog,
            &target,
            &["readme".to_owned()],
            &BTreeMap::new(),
            false,
        )
        .unwrap_err();

        assert!(error.contains("project_name"));
        assert!(!target.exists());
    }
}
