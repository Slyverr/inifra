use clap_complete::{ArgValueCompleter, CompletionCandidate};
use inifra_core::{catalog::Catalog, presets};
use std::{collections::BTreeSet, ffi::OsStr, path::Path};

pub(crate) fn source_name_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(prefix) = current.to_str() else {
            return Vec::new();
        };

        let Ok(project_dir) = std::env::current_dir() else {
            return Vec::new();
        };

        let registry = inifra_core::source_registry::SourceRegistry::load(
            &project_dir.join(".inifra/sources.toml"),
        );
        let Ok(registry) = registry else {
            return Vec::new();
        };
        source_names(&registry, prefix)
            .into_iter()
            .map(CompletionCandidate::new)
            .collect()
    })
}

pub(crate) fn template_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(prefix) = current.to_str() else {
            return Vec::new();
        };

        let Ok(config_dir) = crate::config::user::config_dir() else {
            return Vec::new();
        };

        let Ok(project_dir) = std::env::current_dir() else {
            return Vec::new();
        };

        let Ok((catalog, _bundled)) =
            crate::config::user::load_project_catalog(None, config_dir.as_deref(), &project_dir)
        else {
            return Vec::new();
        };
        template_ids(&catalog, prefix)
            .into_iter()
            .map(CompletionCandidate::new)
            .collect()
    })
}

pub(crate) fn preset_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(prefix) = current.to_str() else {
            return Vec::new();
        };

        let Ok(config_dir) = crate::config::user::config_dir() else {
            return Vec::new();
        };

        let Some(config_dir) = config_dir else {
            return Vec::new();
        };

        let Ok(project_dir) = std::env::current_dir() else {
            return Vec::new();
        };

        let Ok((catalog, _bundled)) =
            crate::config::user::load_project_catalog(None, Some(&config_dir), &project_dir)
        else {
            return Vec::new();
        };
        preset_ids(&config_dir, &catalog, prefix)
            .into_iter()
            .map(CompletionCandidate::new)
            .collect()
    })
}

pub(crate) fn input_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(prefix) = current.to_str() else {
            return Vec::new();
        };

        let Ok(config_dir) = crate::config::user::config_dir() else {
            return Vec::new();
        };

        let Ok(project_dir) = std::env::current_dir() else {
            return Vec::new();
        };

        let Ok((catalog, _bundled)) =
            crate::config::user::load_project_catalog(None, config_dir.as_deref(), &project_dir)
        else {
            return Vec::new();
        };
        input_assignments(&catalog, prefix)
            .into_iter()
            .map(CompletionCandidate::new)
            .collect()
    })
}

pub(super) fn template_ids(catalog: &Catalog, prefix: &str) -> Vec<String> {
    catalog
        .packs
        .iter()
        .filter(|pack| pack.id.starts_with(prefix))
        .map(|pack| pack.id.clone())
        .collect()
}

pub(super) fn source_names(
    registry: &inifra_core::source_registry::SourceRegistry,
    prefix: &str,
) -> Vec<String> {
    registry
        .sources
        .keys()
        .filter(|name| name.starts_with(prefix))
        .cloned()
        .collect()
}

pub(super) fn preset_ids(config_dir: &Path, catalog: &Catalog, prefix: &str) -> Vec<String> {
    presets::load_all(&presets::store_path(config_dir), catalog)
        .unwrap_or_default()
        .into_iter()
        .map(|preset| preset.id)
        .filter(|id| id.starts_with(prefix))
        .collect()
}

pub(super) fn input_assignments(catalog: &Catalog, prefix: &str) -> Vec<String> {
    catalog
        .packs
        .iter()
        .filter_map(|pack| catalog.input_definitions_for(pack).ok())
        .flat_map(|definitions| definitions.into_keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| format!("{name}="))
        .filter(|name| name.starts_with(prefix))
        .collect()
}
