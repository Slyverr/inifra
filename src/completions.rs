use std::ffi::OsStr;
use std::path::PathBuf;

use clap_complete::engine::CompletionCandidate;

pub(crate) fn template_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = current.to_str() else {
        return Vec::new();
    };
    let arguments = completion_arguments();
    let project_directory = project_directory_for(&arguments);
    let Ok(templates) = crate::templates::discover_for(project_directory.as_deref()) else {
        return Vec::new();
    };

    templates
        .into_iter()
        .filter(|template| template.id.starts_with(prefix))
        .map(|template| CompletionCandidate::new(template.id))
        .collect()
}

pub(crate) fn preset_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = current.to_str() else {
        return Vec::new();
    };
    let Ok(presets) = crate::presets::list() else {
        return Vec::new();
    };

    presets
        .into_iter()
        .filter(|preset| preset.name.starts_with(prefix))
        .map(|preset| CompletionCandidate::new(preset.name))
        .collect()
}

pub(crate) fn input_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = current.to_str() else {
        return Vec::new();
    };
    if prefix.contains('=') {
        return Vec::new();
    }

    let Ok(selected_names) = selected_input_names() else {
        return Vec::new();
    };

    selected_names
        .into_iter()
        .map(|name| format!("{name}="))
        .filter(|name| name.starts_with(prefix))
        .map(CompletionCandidate::new)
        .collect()
}

fn selected_input_names() -> anyhow::Result<Vec<String>> {
    let arguments = completion_arguments();

    let Some(command) = arguments.first().map(String::as_str) else {
        return Ok(Vec::new());
    };
    let command = if command == "preset" && arguments.get(1).is_some_and(|arg| arg == "save") {
        "preset-save"
    } else {
        command
    };
    let mut template_ids = Vec::new();
    let mut preset_name = None;
    match command {
        "generate" => template_ids.extend(arguments.get(1).cloned()),
        "init" | "preset-save" => {
            let mut index = if command == "init" { 1 } else { 2 };
            while index < arguments.len() {
                let argument = &arguments[index];
                if argument == "--template" {
                    if let Some(template) = arguments.get(index + 1) {
                        template_ids.push(template.clone());
                    }
                    index += 2;
                } else if let Some(template) = argument.strip_prefix("--template=") {
                    template_ids.push(template.to_owned());
                    index += 1;
                } else if argument == "--preset" && command == "init" {
                    preset_name = arguments.get(index + 1).cloned();
                    index += 2;
                } else if let Some(preset) = argument.strip_prefix("--preset=") {
                    preset_name = Some(preset.to_owned());
                    index += 1;
                } else {
                    index += 1;
                }
            }
        }
        _ => return Ok(Vec::new()),
    }
    if let Some(name) = preset_name
        && let Ok(preset) = crate::presets::load(&name)
    {
        template_ids.extend(preset.templates);
    }

    let project_directory = project_directory_for(&arguments);
    let templates = crate::templates::discover_for(project_directory.as_deref())?;
    let names = templates
        .into_iter()
        .filter(|template| template_ids.contains(&template.id))
        .flat_map(|template| template.inputs)
        .collect::<std::collections::BTreeSet<_>>();
    Ok(names.into_iter().collect())
}

fn completion_arguments() -> Vec<String> {
    let mut arguments = std::env::args_os()
        .skip(1)
        .filter_map(|argument| argument.into_string().ok())
        .collect::<Vec<_>>();
    if let Some(separator) = arguments.iter().position(|argument| argument == "--") {
        arguments.drain(..=separator);
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "inifra")
    {
        arguments.remove(0);
    }
    arguments
}

fn project_directory_for(arguments: &[String]) -> Option<PathBuf> {
    let first = arguments.first()?.as_str();
    let command = if first == "preset" && arguments.get(1).is_some_and(|arg| arg == "save") {
        "preset-save"
    } else {
        first
    };
    let mut path = None;
    let mut index = 1;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--path" || argument == "-p" {
            path = arguments.get(index + 1).map(PathBuf::from);
            index += 2;
        } else if let Some(value) = argument.strip_prefix("--path=") {
            path = Some(PathBuf::from(value));
            index += 1;
        } else {
            index += 1;
        }
    }

    match command {
        "generate" | "init" => Some(path.unwrap_or_else(|| PathBuf::from("."))),
        "list" | "validate" | "edit" => path,
        "preset-save" => Some(path.unwrap_or_else(|| PathBuf::from("."))),
        _ => None,
    }
}
