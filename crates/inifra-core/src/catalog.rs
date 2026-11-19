//! Discover, load, and validate categorized template packs.

use crate::{
    inputs::{self, InputDefinition},
    output_rules::{self, OutputRule, ResolvedPackOutput},
    plan, render, safe_fs, sources,
};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

static BUNDLED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../templates");

/// Extracts the compile-time bundled catalog into an existing private directory.
///
/// # Errors
/// Returns an error when catalog files cannot be written.
pub fn extract_bundled(destination: &Path) -> Result<(), std::io::Error> {
    BUNDLED.extract(destination)
}

/// A loaded catalog with its category and shared input metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Catalog {
    /// Definitions shared across all categories.
    pub inputs: BTreeMap<String, InputDefinition>,
    /// Categories ordered by their directory IDs.
    pub categories: BTreeMap<String, Category>,
    /// Template packs ordered by category and ID.
    pub packs: Vec<Pack>,
}

/// A category and the metadata used to present and configure its packs.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Category {
    /// Category directory ID.
    pub id: String,
    /// Display name from `categories.toml`, or a readable form of the ID.
    pub name: String,
    /// Definitions shared by packs in this category.
    pub inputs: BTreeMap<String, InputDefinition>,
}

/// A discovered template and its location in the catalog.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Pack {
    /// Globally unique pack identifier.
    pub id: String,
    /// Category directory ID.
    pub category: String,
    /// Human-readable summary.
    pub description: String,
    /// Pack directory containing its sources.
    pub directory: PathBuf,
    /// Names of shared inputs used by this pack.
    pub inputs: Vec<String>,
    /// Input definitions used only by this pack.
    pub local_inputs: BTreeMap<String, InputDefinition>,
    /// Intentionally empty output directories declared by this pack.
    pub directories: Vec<PathBuf>,
    /// Optional output path overrides and co-selected alternatives.
    pub output_rules: Vec<OutputRule>,
    /// Optional paths to outputs from other template packs.
    pub references: BTreeMap<String, PackReference>,
}

/// A source output that a pack may refer to when its target is selected too.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PackReference {
    /// ID of the referenced template pack.
    pub template: String,
    /// Source path within the referenced pack.
    pub source: PathBuf,
}

/// A catalog discovery or validation error.
#[derive(Debug)]
pub struct CatalogError(String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for CatalogError {}

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
            let name = category_names
                .get(&category_id)
                .cloned()
                .unwrap_or_else(|| readable_category_name(&category_id));
            categories.insert(
                category_id.clone(),
                Category {
                    id: category_id.clone(),
                    name,
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
                        if !valid_identifier(&definition.template) {
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
                if !ids.contains(&reference.template) {
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

    /// Returns a pack's effective input definitions after scope precedence is applied.
    ///
    /// For each shared input, a category definition overrides the catalog-wide definition.
    /// Pack-local definitions are then included for that pack.
    ///
    /// # Errors
    /// Returns an error when a pack names an undefined shared input or its category is missing.
    pub fn input_definitions_for(
        &self,
        pack: &Pack,
    ) -> Result<BTreeMap<String, InputDefinition>, CatalogError> {
        let category = self
            .categories
            .get(&pack.category)
            .ok_or_else(|| CatalogError(format!("unknown category {:?}", pack.category)))?;
        let mut definitions = BTreeMap::new();
        for name in &pack.inputs {
            let definition = category
                .inputs
                .get(name)
                .or_else(|| self.inputs.get(name))
                .ok_or_else(|| {
                    CatalogError(format!(
                        "template {:?} references unknown input {name:?}",
                        pack.id
                    ))
                })?;
            definitions.insert(name.clone(), definition.clone());
        }
        definitions.extend(pack.local_inputs.clone());
        Ok(definitions)
    }

    /// Resolves final source destinations for selected template IDs.
    ///
    /// Co-selected alternatives are used only when selected packs declare the same group and
    /// their default destinations collide. The returned source paths stay relative to each pack.
    ///
    /// # Errors
    /// Returns an error for an empty or invalid selection, missing sources, or conflicting paths.
    pub fn resolve_outputs(
        &self,
        selected: &[String],
    ) -> Result<Vec<ResolvedPackOutput>, CatalogError> {
        if selected.is_empty() {
            return Err(CatalogError("select at least one template".into()));
        }
        let mut seen = BTreeSet::new();
        let mut packs = Vec::with_capacity(selected.len());
        for id in selected {
            if !seen.insert(id) {
                return Err(CatalogError(format!("duplicate template {id:?}")));
            }
            let pack = self
                .packs
                .iter()
                .find(|pack| &pack.id == id)
                .ok_or_else(|| CatalogError(format!("unknown template {id:?}")))?;
            packs.push(pack);
        }
        self.resolve_outputs_for_packs(&packs)
    }

    /// Validates all pack inputs, sources, paths, and template syntax.
    ///
    /// # Errors
    /// Returns an error when a pack references missing inputs, has invalid sources, conflicting
    /// outputs, or contains invalid template syntax.
    pub fn validate(&self) -> Result<(), CatalogError> {
        for pack in &self.packs {
            self.input_definitions_for(pack)?;
            let outputs = self.resolve_outputs_for_packs(&[pack])?;
            for output in outputs {
                if output.source.render {
                    render::validate_template(&pack.directory.join(output.source.source))
                        .map_err(|error| CatalogError(error.to_string()))?;
                }
            }
        }
        self.validate_references_for_packs(&self.packs.iter().collect::<Vec<_>>())?;
        Ok(())
    }

    pub(crate) fn resolve_references_for_packs(
        &self,
        packs: &[&Pack],
        outputs: &[ResolvedPackOutput],
    ) -> Result<BTreeMap<String, BTreeMap<String, String>>, CatalogError> {
        self.validate_references_for_packs(packs)?;
        let selected: BTreeSet<_> = packs.iter().map(|pack| pack.id.as_str()).collect();
        packs
            .iter()
            .map(|pack| {
                let references = pack
                    .references
                    .iter()
                    .map(|(name, reference)| {
                        let output = if selected.contains(reference.template.as_str()) {
                            outputs
                                .iter()
                                .find(|output| {
                                    output.template_id == reference.template
                                        && output.source.source == reference.source
                                })
                                .ok_or_else(|| {
                                    CatalogError(format!(
                                        "template {:?} reference {name:?} could not resolve output from {:?} source {}",
                                        pack.id,
                                        reference.template,
                                        reference.source.display()
                                    ))
                                })?
                                .output
                                .to_string_lossy()
                                .replace('\\', "/")
                        } else {
                            String::new()
                        };
                        Ok((name.clone(), output))
                    })
                    .collect::<Result<_, CatalogError>>()?;
                Ok((pack.id.clone(), references))
            })
            .collect()
    }

    pub(crate) fn resolve_outputs_for_packs(
        &self,
        packs: &[&Pack],
    ) -> Result<Vec<ResolvedPackOutput>, CatalogError> {
        let mut discovered = Vec::with_capacity(packs.len());
        let mut directories = BTreeSet::new();
        for pack in packs {
            let files = sources::discover(&pack.directory)
                .map_err(|error| CatalogError(error.to_string()))?;
            if files.is_empty() && pack.directories.is_empty() {
                return Err(CatalogError(format!(
                    "template {:?} has no files or directories",
                    pack.id
                )));
            }
            directories.extend(pack.directories.iter().cloned());
            discovered.push((*pack, files));
        }
        let source_sets: Vec<_> = discovered
            .iter()
            .map(|(pack, files)| (*pack, files.as_slice()))
            .collect();
        let outputs = output_rules::resolve(&source_sets).map_err(CatalogError)?;
        plan::validate_paths(
            outputs
                .iter()
                .map(|output| output.output.as_path())
                .chain(directories.iter().map(PathBuf::as_path)),
        )
        .map_err(|error| CatalogError(error.to_string()))?;
        Ok(outputs)
    }

    fn validate_references_for_packs(&self, packs: &[&Pack]) -> Result<(), CatalogError> {
        for pack in packs {
            for (name, reference) in &pack.references {
                let target = self
                    .packs
                    .iter()
                    .find(|target| target.id == reference.template)
                    .ok_or_else(|| {
                        CatalogError(format!(
                            "template {:?} reference {name:?} targets unknown template {:?}",
                            pack.id, reference.template
                        ))
                    })?;
                let target_sources = sources::discover(&target.directory)
                    .map_err(|error| CatalogError(error.to_string()))?;
                if !target_sources
                    .iter()
                    .any(|source| source.source == reference.source)
                {
                    return Err(CatalogError(format!(
                        "template {:?} reference {name:?} points to missing source {} in template {:?}",
                        pack.id,
                        reference.source.display(),
                        target.id
                    )));
                }
            }
        }
        Ok(())
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

fn valid_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
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
