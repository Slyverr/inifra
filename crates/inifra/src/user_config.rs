use inifra_core::catalog::Catalog;
use std::{env, error::Error, fs, io, path::Path, path::PathBuf};
use tempfile::TempDir;

/// Returns the platform's Inifra configuration directory, when a home directory is available.
pub(crate) fn config_dir() -> Result<Option<PathBuf>, io::Error> {
    if let Some(path) = env::var_os("INIFRA_CONFIG_DIR") {
        if path.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "INIFRA_CONFIG_DIR cannot be empty",
            ));
        }
        return Ok(Some(path.into()));
    }
    Ok(directories::BaseDirs::new().map(|directories| directories.config_dir().join("inifra")))
}

/// Loads either an explicitly selected catalog or the bundled catalog with a user overlay.
///
/// The temporary extraction must remain alive while the catalog is in use because bundled packs
/// retain paths into that directory.
pub(crate) fn load_catalog(
    explicit_catalog: Option<&Path>,
    config_dir: Option<&Path>,
) -> Result<(Catalog, Option<TempDir>), Box<dyn Error>> {
    if let Some(path) = explicit_catalog {
        return Ok((Catalog::load(path)?, None));
    }

    let bundled_dir = tempfile::tempdir()?;
    inifra_core::catalog::extract_bundled(bundled_dir.path())?;
    let bundled = Catalog::load(bundled_dir.path())?;
    let Some(config_dir) = config_dir else {
        return Ok((bundled, Some(bundled_dir)));
    };
    let user_templates = config_dir.join("templates");

    match fs::metadata(&user_templates) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((bundled, Some(bundled_dir))),
        Err(error) => Err(format!("{}: {error}", user_templates.display()).into()),
        Ok(metadata) if metadata.is_dir() => {
            let overlay = Catalog::load_overlay(&user_templates)?;
            Ok((bundled.with_overlay(overlay)?, Some(bundled_dir)))
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{}: user templates path must be a directory",
                user_templates.display()
            ),
        )
        .into()),
    }
}

/// Loads the default catalog layers for a project rooted at `project_dir`.
///
/// Project packs in `.inifra/templates` are the final layer, so they can replace a bundled or
/// personal pack with the same ID. An explicit `--catalog` remains an isolated catalog.
pub(crate) fn load_project_catalog(
    explicit_catalog: Option<&Path>,
    config_dir: Option<&Path>,
    project_dir: &Path,
) -> Result<(Catalog, Option<TempDir>), Box<dyn Error>> {
    let (catalog, bundled_dir) = load_catalog(explicit_catalog, config_dir)?;
    if explicit_catalog.is_some() {
        return Ok((catalog, bundled_dir));
    }

    let inifra_dir = project_dir.join(".inifra");
    match fs::symlink_metadata(&inifra_dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((catalog, bundled_dir)),
        Err(error) => Err(format!("{}: {error}", inifra_dir.display()).into()),
        Ok(metadata) if metadata.file_type().is_dir() => {
            let project_templates = inifra_dir.join("templates");
            match fs::symlink_metadata(&project_templates) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((catalog, bundled_dir)),
                Err(error) => Err(format!("{}: {error}", project_templates.display()).into()),
                Ok(metadata) if metadata.file_type().is_dir() => {
                    let overlay = Catalog::load_overlay(&project_templates)?;
                    Ok((catalog.with_overlay(overlay)?, bundled_dir))
                }
                Ok(_) => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{}: project templates path must be a directory",
                        project_templates.display()
                    ),
                )
                .into()),
            }
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{}: project configuration path must be a directory",
                inifra_dir.display()
            ),
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_pack(root: &Path, category: &str, id: &str, description: &str) {
        let directory = root.join(category).join(id);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("template.toml"),
            format!("schema_version = 2\ndescription = '{description}'\n"),
        )
        .unwrap();
        fs::write(directory.join("GUIDE.md"), "Guide\n").unwrap();
    }

    #[test]
    fn default_catalog_adds_user_packs_and_keeps_bundled_sources_alive() {
        let root = tempfile::tempdir().unwrap();
        write_pack(
            &root.path().join("templates"),
            "custom",
            "guide",
            "User guide",
        );

        let (catalog, bundled_dir) = load_catalog(None, Some(root.path())).unwrap();

        assert!(catalog.packs.iter().any(|pack| pack.id == "agents"));
        assert!(catalog.packs.iter().any(|pack| pack.id == "guide"));
        assert!(bundled_dir.is_some());
        assert!(
            catalog
                .packs
                .iter()
                .find(|pack| pack.id == "agents")
                .unwrap()
                .directory
                .join("AGENTS.md.j2")
                .is_file()
        );
    }

    #[test]
    fn explicit_catalog_does_not_merge_user_packs() {
        let explicit = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        write_pack(explicit.path(), "custom", "only-explicit", "Explicit");
        write_pack(
            &config.path().join("templates"),
            "custom",
            "only-user",
            "User",
        );

        let (catalog, bundled_dir) =
            load_catalog(Some(explicit.path()), Some(config.path())).unwrap();

        assert_eq!(catalog.packs.len(), 1);
        assert_eq!(catalog.packs[0].id, "only-explicit");
        assert!(bundled_dir.is_none());
    }

    #[test]
    fn malformed_user_catalog_is_reported() {
        let config = tempfile::tempdir().unwrap();
        let directory = config.path().join("templates/custom/bad");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("template.toml"),
            "schema_version = 99\ndescription = 'Bad schema'\n",
        )
        .unwrap();

        let error = load_catalog(None, Some(config.path()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported schema version"), "{error}");
    }

    #[test]
    fn project_catalog_is_the_last_overlay_and_can_reference_bundled_packs() {
        let project = tempfile::tempdir().unwrap();
        write_pack(
            &project.path().join(".inifra/templates"),
            "project-files",
            "agents",
            "Project agent instructions",
        );
        write_pack(
            &project.path().join(".inifra/templates"),
            "project-files",
            "team-guide",
            "Team guide",
        );
        fs::write(
            project
                .path()
                .join(".inifra/templates/project-files/team-guide/template.toml"),
            "schema_version = 2\ndescription = 'Team guide'\n\n[references.agents]\ntemplate = 'agents'\nsource = 'GUIDE.md'\n",
        )
        .unwrap();

        let (catalog, _bundled_dir) = load_project_catalog(None, None, project.path()).unwrap();
        let agents = catalog
            .packs
            .iter()
            .find(|pack| pack.id == "agents")
            .unwrap();
        assert_eq!(agents.description, "Project agent instructions");
        assert!(
            agents
                .directory
                .starts_with(project.path().join(".inifra/templates"))
        );
        assert!(catalog.packs.iter().any(|pack| pack.id == "team-guide"));
        catalog.validate().unwrap();
    }

    #[test]
    fn explicit_catalog_does_not_merge_project_packs() {
        let project = tempfile::tempdir().unwrap();
        let explicit = tempfile::tempdir().unwrap();
        write_pack(
            &project.path().join(".inifra/templates"),
            "custom",
            "project-only",
            "Project-only pack",
        );
        write_pack(explicit.path(), "custom", "explicit-only", "Explicit pack");

        let (catalog, _) =
            load_project_catalog(Some(explicit.path()), None, project.path()).unwrap();

        assert_eq!(catalog.packs.len(), 1);
        assert_eq!(catalog.packs[0].id, "explicit-only");
    }

    #[test]
    fn project_catalog_does_not_search_parent_directories() {
        let parent = tempfile::tempdir().unwrap();
        write_pack(
            &parent.path().join(".inifra/templates"),
            "custom",
            "parent-only",
            "Parent pack",
        );
        let child = parent.path().join("child");
        fs::create_dir(&child).unwrap();

        let (catalog, _) = load_project_catalog(None, None, &child).unwrap();

        assert!(!catalog.packs.iter().any(|pack| pack.id == "parent-only"));
    }

    #[test]
    fn malformed_project_catalog_is_not_silently_ignored() {
        let project = tempfile::tempdir().unwrap();
        let directory = project.path().join(".inifra/templates/project/bad");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("template.toml"),
            "schema_version = 99\ndescription = 'Unsupported'\n",
        )
        .unwrap();

        let error = load_project_catalog(None, None, project.path())
            .unwrap_err()
            .to_string();

        assert!(error.contains("unsupported schema version"), "{error}");
    }
}
