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
}
