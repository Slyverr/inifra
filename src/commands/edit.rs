use std::fs;
use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{output, templates};

pub(super) fn run(id: &str) -> Result<()> {
    templates::validate_id(id)?;
    let catalog = templates::discover()?;
    let pack_path = templates::local_pack_path(id)?;
    if let Some(template) = catalog.iter().find(|template| template.id == id) {
        if template.origin == templates::TemplateOrigin::Builtin {
            copy_builtin_to_config(id, &pack_path)?;
        }
        let files = templates::source_files(&pack_path)?;
        return output::open_files_in_editor(&files)
            .with_context(|| format!("could not edit template {id:?}"));
    }

    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        bail!("unknown template {id:?}; run `inifra edit {id}` in a terminal to create it");
    }
    let create = dialoguer::Confirm::new()
        .with_prompt(format!("Create a new template named {id:?}?"))
        .default(false)
        .interact()
        .context("could not confirm template creation")?;
    if !create {
        return Ok(());
    }

    create_template_pack(id, &pack_path)?;
    let files = templates::source_files(&pack_path)?;
    output::open_files_in_editor(&files).with_context(|| format!("could not edit template {id:?}"))
}

fn copy_builtin_to_config(id: &str, destination: &Path) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => bail!(
            "local template path already exists: {}",
            destination.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not inspect template path {}", destination.display())
            });
        }
    }
    templates::copy_builtin(id, destination)
        .with_context(|| format!("could not copy built-in template {id:?} to user config"))
}

fn create_template_pack(id: &str, destination: &Path) -> Result<()> {
    let root = destination
        .parent()
        .context("template pack path has no parent directory")?;
    fs::create_dir_all(root)
        .with_context(|| format!("could not create template directory {}", root.display()))?;
    let root_metadata = fs::symlink_metadata(root)
        .with_context(|| format!("could not inspect template directory {}", root.display()))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        bail!(
            "template path must be a regular directory: {}",
            root.display()
        );
    }
    fs::create_dir(destination)
        .with_context(|| format!("could not create template pack {}", destination.display()))?;
    let manifest = format!(
        "schema_version = 1\nid = {id:?}\ndescription = \"Custom template\"\n\n[[files]]\nsource = \"template.txt.j2\"\noutput = \"output.txt\"\n"
    );
    fs::write(destination.join("template.toml"), manifest)
        .with_context(|| format!("could not create template manifest for {id:?}"))?;
    fs::write(destination.join("template.txt.j2"), "")
        .with_context(|| format!("could not create template source for {id:?}"))?;
    Ok(())
}
