use super::{
    Catalog, CatalogError, Category, Pack, PackReference, valid_identifier, valid_template_id,
};
use crate::{
    inputs::{self, InputDefinition},
    output_rules, plan, safe_fs,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryFile {
    schema_version: u32,
    #[serde(default)]
    categories: BTreeMap<String, CategoryOverride>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryOverride {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    description: String,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    local_inputs: BTreeMap<String, InputDefinition>,
    #[serde(default)]
    directories: Vec<String>,
    #[serde(default)]
    output_rules: Vec<output_rules::OutputRuleDefinition>,
    #[serde(default)]
    references: BTreeMap<String, ReferenceDefinition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceDefinition {
    template: String,
    source: String,
}

impl Catalog {
    /// Loads category metadata, shared inputs, and pack manifests from a catalog root.
    ///
    /// `inputs.toml` and `categories.toml` are optional. Missing input scopes are treated as
    /// empty; malformed files and unsupported schema versions are errors.
    ///
    /// # Errors
    /// Returns an error for invalid IDs, duplicate IDs or input declarations, unreadable files,
    /// malformed metadata, or unsupported schemas.
    pub fn load(root: &Path) -> Result<Self, CatalogError> {
        Self::load_with_external_references(root, false)
    }

    /// Loads a catalog layer whose references may target another catalog layer.
    ///
    /// Use [`Catalog::with_overlay`] to combine this layer with its base catalog; the combined
    /// catalog validates every reference against the final set of packs.
    ///
    /// # Errors
    /// Returns an error for invalid IDs, duplicate IDs or input declarations, unreadable files,
    /// malformed metadata, or unsupported schemas.
    pub fn load_overlay(root: &Path) -> Result<Self, CatalogError> {
        Self::load_with_external_references(root, true)
    }
    fn load_with_external_references(
        root: &Path,
        allow_external_references: bool,
    ) -> Result<Self, CatalogError> {
        let inputs = inputs::load_optional(&root.join("inputs.toml"))
            .map_err(|error| CatalogError(error.to_string()))?;
        let category_names = load_category_names(root)?;
        let category_paths = directories(root)?;
        let category_ids: BTreeSet<_> = category_paths
            .iter()
            .map(|path| identifier(path))
            .collect::<Result<_, _>>()?;
        for id in category_names.keys() {
            if !category_ids.contains(id) {
                return Err(CatalogError(format!(
                    "categories.toml references unknown category {id:?}"
                )));
            }
        }

        let mut categories = BTreeMap::new();
        let mut packs = Vec::new();
        let mut ids = BTreeSet::new();
        for category_path in category_paths {
            let category_id = identifier(&category_path)?;
            let category_inputs = inputs::load_optional(&category_path.join("inputs.toml"))
                .map_err(|error| CatalogError(error.to_string()))?;
            let has_display_name_override = category_names.contains_key(&category_id);
            let name = category_names
                .get(&category_id)
                .cloned()
                .unwrap_or_else(|| readable_category_name(&category_id));
            categories.insert(
                category_id.clone(),
                Category {
                    id: category_id.clone(),
                    name,
                    has_display_name_override,
                    inputs: category_inputs,
                },
            );

            for directory in directories(&category_path)? {
                let id = identifier(&directory)?;
                if !ids.insert(id.clone()) {
                    return Err(CatalogError(format!("duplicate template ID {id:?}")));
                }

                let path = directory.join("template.toml");
                let text = safe_fs::read_regular_file(&path, "manifest")
                    .map_err(|error| CatalogError(format!("{}: {error}", path.display())))?;
                let manifest: Manifest = toml::from_str(&text)
                    .map_err(|error| CatalogError(format!("{}: {error}", path.display())))?;
                if manifest.schema_version != 2 {
                    return Err(CatalogError(format!(
                        "{}: unsupported schema version {}",
                        path.display(),
                        manifest.schema_version
                    )));
                }
                if manifest.description.trim().is_empty() {
                    return Err(CatalogError(format!(
                        "{}: description is empty",
                        path.display()
                    )));
                }

                let mut input_names = BTreeSet::new();
                for name in &manifest.inputs {
                    if !input_names.insert(name) {
                        return Err(CatalogError(format!(
                            "{}: duplicate input {name:?}",
                            path.display()
                        )));
                    }
                }
                inputs::validate_definitions(&manifest.local_inputs, &path)
                    .map_err(|error| CatalogError(error.to_string()))?;
                if let Some(name) = manifest
                    .local_inputs
                    .keys()
                    .find(|name| input_names.contains(name))
                {
                    return Err(CatalogError(format!(
                        "{}: input {name:?} cannot be both shared and pack-local",
                        path.display()
                    )));
                }

                let directories: Vec<_> = manifest
                    .directories
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();
                plan::validate_paths(directories.iter().map(PathBuf::as_path))
                    .map_err(|error| CatalogError(format!("{}: {error}", path.display())))?;
                let output_rules =
                    output_rules::parse(manifest.output_rules, &path).map_err(CatalogError)?;
                let references = manifest
                    .references
                    .into_iter()
                    .map(|(name, definition)| {
                        let source = PathBuf::from(definition.source);
                        if !inputs::valid_name(&name) {
                            return Err(CatalogError(format!(
                                "{}: invalid reference name {name:?}",
                                path.display()
                            )));
                        }
                        plan::validate_paths(std::iter::once(source.as_path())).map_err(
                            |error| CatalogError(format!("{}: {error}", path.display())),
                        )?;
                        if !valid_template_id(&definition.template) {
                            return Err(CatalogError(format!(
                                "{}: invalid referenced template ID {:?}",
                                path.display(),
                                definition.template
                            )));
                        }
                        Ok((
                            name,
                            PackReference {
                                template: definition.template,
                                source,
                            },
                        ))
                    })
                    .collect::<Result<_, CatalogError>>()?;
                packs.push(Pack {
                    id,
                    category: category_id.clone(),
                    description: manifest.description,
                    directory,
                    inputs: manifest.inputs,
                    local_inputs: manifest.local_inputs,
                    directories,
                    output_rules,
                    references,
                });
            }
        }

        for pack in &packs {
            for (name, reference) in &pack.references {
                if reference.template == pack.id {
                    return Err(CatalogError(format!(
                        "template {:?} reference {name:?} must target another template",
                        pack.id
                    )));
                }
                if !allow_external_references && !ids.contains(&reference.template) {
                    return Err(CatalogError(format!(
                        "template {:?} reference {name:?} targets unknown template {:?}",
                        pack.id, reference.template
                    )));
                }
            }
        }

        Ok(Self {
            inputs,
            categories,
            packs,
        })
    }
}

fn load_category_names(root: &Path) -> Result<BTreeMap<String, String>, CatalogError> {
    let path = root.join("categories.toml");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(CatalogError(format!("{}: {error}", path.display()))),
    };
    if !metadata.file_type().is_file() {
        return Err(CatalogError(format!(
            "{}: category metadata must be a regular file",
            path.display()
        )));
    }

    let text = safe_fs::read_regular_file(&path, "category metadata")
        .map_err(|error| CatalogError(format!("{}: {error}", path.display())))?;
    let file: CategoryFile = toml::from_str(&text)
        .map_err(|error| CatalogError(format!("{}: {error}", path.display())))?;
    if file.schema_version != 1 {
        return Err(CatalogError(format!(
            "{}: unsupported category schema version {}",
            path.display(),
            file.schema_version
        )));
    }

    for (id, category) in &file.categories {
        if !valid_identifier(id)
            || category.name.trim().is_empty()
            || category.name.chars().any(char::is_control)
        {
            return Err(CatalogError(format!(
                "{}: invalid category definition {id:?}",
                path.display()
            )));
        }
    }
    Ok(file
        .categories
        .into_iter()
        .map(|(id, category)| (id, category.name))
        .collect())
}

fn directories(path: &Path) -> Result<Vec<PathBuf>, CatalogError> {
    let entries =
        fs::read_dir(path).map_err(|error| CatalogError(format!("{}: {error}", path.display())))?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| CatalogError(format!("{}: {error}", path.display())))?;
        let kind = entry
            .file_type()
            .map_err(|error| CatalogError(format!("{}: {error}", entry.path().display())))?;
        if kind.is_symlink() {
            return Err(CatalogError(format!(
                "{}: symlinks are not supported",
                entry.path().display()
            )));
        }
        if kind.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

fn identifier(path: &Path) -> Result<String, CatalogError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !valid_identifier(name) {
        return Err(CatalogError(format!("{}: invalid ID", path.display())));
    }
    Ok(name.to_owned())
}

fn readable_category_name(id: &str) -> String {
    id.split('-')
        .map(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
