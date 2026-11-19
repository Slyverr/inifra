use inifra_core::{catalog::Catalog, presets, sources, user_templates};
use inifra_tui::{FormOutcome, TextField, presentation};
use std::{
    error::Error,
    io::{self, IsTerminal},
    path::Path,
    process::Command,
};

pub(crate) fn edit(
    catalog: &Catalog,
    config_dir: Option<&Path>,
    project_dir: &Path,
    id: &str,
) -> Result<(), Box<dyn Error>> {
    user_templates::validate_id(id)?;
    if let Some(pack) = catalog.packs.iter().find(|pack| pack.id == id) {
        let project_templates = project_dir.join(".inifra/templates");
        if pack.directory.starts_with(&project_templates) {
            return open_pack(&pack.directory);
        }

        let templates_root = require_config(config_dir)?.join("templates");
        if pack.directory.starts_with(&templates_root) {
            return open_pack(&pack.directory);
        }

        let destination = templates_root.join(&pack.category).join(id);
        user_templates::copy_pack(&pack.directory, &destination).map_err(|error| {
            io::Error::other(format!("could not copy template {id:?}: {error}"))
        })?;
        println!(
            "{} {}",
            presentation::paint("Copied template to", presentation::Tone::Success, false),
            presentation::paint(
                presentation::terminal_text(&destination.display().to_string()),
                presentation::Tone::Muted,
                false
            )
        );
        return open_pack(&destination);
    }

    let templates_root = require_config(config_dir)?.join("templates");

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown template {id:?}; run `inifra edit {id}` in a terminal to create it"),
        )
        .into());
    }
    if inifra_tui::run_confirmation("Create template", &format!("Create {id:?}?"), false)?
        != Some(true)
    {
        return Ok(());
    }
    let values = match inifra_tui::run_form(
        "New template",
        vec![
            TextField {
                name: "category".into(),
                label: "Category ID".into(),
                required: true,
                default: Some("personal".into()),
            },
            TextField {
                name: "description".into(),
                label: "Short description".into(),
                required: true,
                default: None,
            },
            TextField {
                name: "source".into(),
                label: "First source file".into(),
                required: true,
                default: Some(format!("{id}.md.j2")),
            },
        ],
    )? {
        FormOutcome::Submitted(values) => values,
        FormOutcome::Cancelled => return Ok(()),
    };
    let category = values
        .get("category")
        .ok_or("template category was not collected")?;
    let description = values
        .get("description")
        .ok_or("template description was not collected")?;
    let source = values
        .get("source")
        .ok_or("template source path was not collected")?;
    let pack = user_templates::create_pack(
        &templates_root,
        category,
        id,
        description,
        Path::new(source),
    )?;
    println!(
        "{} {}",
        presentation::paint("Created template", presentation::Tone::Success, false),
        presentation::paint_emphasis(id, presentation::Tone::Template)
    );
    open_pack(&pack)
}

pub(crate) fn rename(
    catalog: &Catalog,
    config_dir: Option<&Path>,
    project_dir: &Path,
    old_id: &str,
    new_id: &str,
) -> Result<(), Box<dyn Error>> {
    user_templates::validate_id(new_id)?;
    let config_dir = require_config(config_dir)?;
    let templates_root = config_dir.join("templates");
    let pack = owned_pack(catalog, &templates_root, old_id)?;
    if catalog.packs.iter().any(|candidate| candidate.id == new_id) {
        return Err(format!("template ID {new_id:?} is already in use").into());
    }
    let preset_root = presets::store_path(config_dir);
    presets::load_all(&preset_root, catalog)?;
    let category = pack.category.as_str();

    user_templates::rename_pack(&templates_root, category, old_id, new_id)?;
    let pack_references = match user_templates::rename_references(&templates_root, old_id, new_id) {
        Ok(count) => count,
        Err(error) => {
            if let Err(rollback) =
                rollback_rename(&templates_root, &preset_root, category, new_id, old_id)
            {
                return Err(format!(
                    "{error}; restoring the original template also failed: {rollback}"
                )
                .into());
            }
            return Err(error.into());
        }
    };
    let preset_references = match presets::rename_template_references(&preset_root, old_id, new_id)
    {
        Ok(count) => count,
        Err(error) => {
            if let Err(rollback) =
                rollback_rename(&templates_root, &preset_root, category, new_id, old_id)
            {
                return Err(format!(
                    "{error}; restoring the original template also failed: {rollback}"
                )
                .into());
            }
            return Err(error.into());
        }
    };

    let validation = (|| {
        let (renamed_catalog, _bundled) =
            crate::user_config::load_project_catalog(None, Some(config_dir), project_dir)?;
        renamed_catalog.validate()?;
        presets::load_all(&preset_root, &renamed_catalog)?;
        Ok::<_, Box<dyn Error>>(())
    })();
    if let Err(error) = validation {
        if let Err(rollback) =
            rollback_rename(&templates_root, &preset_root, category, new_id, old_id)
        {
            return Err(format!(
                "{error}; restoring the original template also failed: {rollback}"
            )
            .into());
        }
        return Err(error);
    }

    let mut details = Vec::new();
    if pack_references > 0 {
        details.push(format!("updated {pack_references} template reference(s)"));
    }
    if preset_references > 0 {
        details.push(format!("updated {preset_references} preset(s)"));
    }
    let suffix = if details.is_empty() {
        String::new()
    } else {
        format!("; {}", details.join(" and "))
    };
    println!(
        "Renamed template {} to {}{suffix}.",
        presentation::paint_emphasis(old_id, presentation::Tone::Muted),
        presentation::paint_emphasis(new_id, presentation::Tone::Template)
    );
    Ok(())
}

pub(crate) fn remove(
    catalog: &Catalog,
    config_dir: Option<&Path>,
    id: &str,
) -> Result<(), Box<dyn Error>> {
    let config_dir = require_config(config_dir)?;
    let templates_root = config_dir.join("templates");
    let pack = owned_pack(catalog, &templates_root, id)?;
    let preset_root = presets::store_path(config_dir);
    let referencing_presets = presets::templates_referencing(&preset_root, catalog, id)?;
    if !referencing_presets.is_empty() {
        return Err(format!(
            "template {id:?} is used by saved presets: {}; remove or update those presets first",
            referencing_presets.join(", ")
        )
        .into());
    }
    let referencing_packs = catalog
        .packs
        .iter()
        .filter(|candidate| candidate.id != id)
        .filter(|candidate| {
            candidate
                .references
                .values()
                .any(|reference| reference.template == id)
        })
        .map(|candidate| candidate.id.as_str())
        .collect::<Vec<_>>();
    if !referencing_packs.is_empty() {
        return Err(format!(
            "template {id:?} is referenced by templates: {}; update them first",
            referencing_packs.join(", ")
        )
        .into());
    }
    user_templates::remove_pack(&templates_root, &pack.category, id)?;
    println!(
        "Removed user template {}.",
        presentation::paint_emphasis(id, presentation::Tone::Template)
    );
    Ok(())
}

fn owned_pack<'a>(
    catalog: &'a Catalog,
    templates_root: &Path,
    id: &str,
) -> Result<&'a inifra_core::catalog::Pack, Box<dyn Error>> {
    let pack = catalog
        .packs
        .iter()
        .find(|pack| pack.id == id)
        .ok_or_else(|| format!("unknown template {id:?}"))?;
    if !pack.directory.starts_with(templates_root) {
        return Err(format!(
            "template {id:?} is not stored in the personal template catalog; manage project templates in `.inifra/templates` and copy bundled templates with `inifra edit {id}` first"
        )
        .into());
    }
    Ok(pack)
}

fn rollback_rename(
    templates_root: &Path,
    preset_root: &Path,
    category: &str,
    new_id: &str,
    old_id: &str,
) -> Result<(), Box<dyn Error>> {
    let mut failures = Vec::new();
    if let Err(error) = presets::rename_template_references(preset_root, new_id, old_id) {
        failures.push(format!("preset references: {error}"));
    }
    if let Err(error) = user_templates::rename_references(templates_root, new_id, old_id) {
        failures.push(format!("template references: {error}"));
    }
    if let Err(error) = user_templates::rename_pack(templates_root, category, new_id, old_id) {
        failures.push(format!("template directory: {error}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other(format!("rollback was incomplete ({})", failures.join("; "))).into())
    }
}

fn require_config(config_dir: Option<&Path>) -> Result<&Path, Box<dyn Error>> {
    config_dir.ok_or_else(|| "could not determine the user config directory".into())
}

fn open_pack(directory: &Path) -> Result<(), Box<dyn Error>> {
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or("set VISUAL or EDITOR to open template files")?;
    let mut editor = shlex::split(&editor).ok_or("could not parse VISUAL or EDITOR")?;
    if editor.is_empty() {
        return Err("VISUAL or EDITOR must name an editor executable".into());
    }
    let executable = editor.remove(0);
    let manifest = directory.join("template.toml");
    let mut files = sources::discover(directory)?
        .into_iter()
        .map(|source| directory.join(source.source))
        .collect::<Vec<_>>();
    files.sort();
    files.insert(0, manifest);
    let status = Command::new(&executable)
        .args(editor)
        .args(files)
        .status()?;
    if !status.success() {
        return Err(format!("editor {executable:?} exited with {status}").into());
    }
    Ok(())
}
