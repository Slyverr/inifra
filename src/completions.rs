use std::ffi::OsStr;

use clap_complete::engine::CompletionCandidate;

pub(crate) fn template_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = current.to_str() else {
        return Vec::new();
    };
    let Ok(templates) = crate::templates::discover() else {
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
