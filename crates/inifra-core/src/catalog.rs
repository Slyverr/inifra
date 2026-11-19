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
    /// Catalog category ID; remote catalog layers prefix it with their source namespace.
    pub id: String,
    /// Display name from `categories.toml`, or a readable form of the ID.
    pub name: String,
    /// Whether `name` was explicitly set in `categories.toml`.
    pub has_display_name_override: bool,
    /// Definitions shared by packs in this category.
    pub inputs: BTreeMap<String, InputDefinition>,
}

/// A discovered template and its location in the catalog.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Pack {
    /// Unique effective catalog ID; remote pack IDs use `<source>/<template>`.
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

    /// Combines one user catalog layer with this base catalog.
    ///
    /// User definitions replace matching root or category input definitions. An explicitly named
    /// user category replaces the base display name, while an omitted name preserves it. A user
    /// pack replaces a base pack with the same ID; other user packs are added. The returned
    /// catalog validates references and outputs against the merged catalog.
    ///
    /// # Errors
    /// Returns an error when the merged catalog contains invalid inputs, references, sources,
    /// template syntax, or outputs.
    pub fn with_overlay(self, overlay: Self) -> Result<Self, CatalogError> {
        self.with_overlays([overlay])
    }

    /// Combines catalog layers in precedence order and validates the complete result once.
    ///
    /// This permits references between overlays that appear later in the sequence, while keeping
    /// later packs and input definitions authoritative when IDs overlap.
    ///
    /// # Errors
    /// Returns an error when the merged catalog contains invalid inputs, references, sources,
    /// template syntax, or outputs.
    pub fn with_overlays(
        mut self,
        overlays: impl IntoIterator<Item = Self>,
    ) -> Result<Self, CatalogError> {
        for overlay in overlays {
            self.inputs.extend(overlay.inputs);

            for (id, mut overlay_category) in overlay.categories {
                if let Some(category) = self.categories.get_mut(&id) {
                    if overlay_category.has_display_name_override {
                        category.name = overlay_category.name;
                        category.has_display_name_override = true;
                    }
                    category.inputs.append(&mut overlay_category.inputs);
                } else {
                    self.categories.insert(id, overlay_category);
                }
            }

            for overlay_pack in overlay.packs {
                if let Some(pack) = self
                    .packs
                    .iter_mut()
                    .find(|pack| pack.id == overlay_pack.id)
                {
                    *pack = overlay_pack;
                } else {
                    self.packs.push(overlay_pack);
                }
            }
        }
        self.packs
            .sort_by(|left, right| (&left.category, &left.id).cmp(&(&right.category, &right.id)));

        self.validate()?;
        Ok(self)
    }

    /// Adds a stable namespace to every pack and category in this catalog layer.
    ///
    /// Shared catalog inputs are moved into each category before namespacing so two sources can
    /// define their own inputs without overriding one another. References to packs in this layer
    /// are qualified too; references to other layers keep their existing IDs.
    ///
    /// # Errors
    /// Returns an error when `namespace` is not a valid catalog ID component.
    pub fn with_namespace(mut self, namespace: &str) -> Result<Self, CatalogError> {
        if !valid_identifier(namespace) {
            return Err(CatalogError(format!(
                "invalid catalog namespace {namespace:?}"
            )));
        }

        let pack_ids: BTreeMap<_, _> = self
            .packs
            .iter()
            .map(|pack| (pack.id.clone(), format!("{namespace}/{}", pack.id)))
            .collect();
        let global_inputs = std::mem::take(&mut self.inputs);
        let mut categories = BTreeMap::new();
        for (_, mut category) in std::mem::take(&mut self.categories) {
            let original_id = category.id.clone();
            category.id = format!("{namespace}/{original_id}");
            category.name = format!("{namespace} / {}", category.name);
            let mut inputs = global_inputs.clone();
            inputs.append(&mut category.inputs);
            category.inputs = inputs;
            categories.insert(category.id.clone(), category);
        }
        self.categories = categories;

        for pack in &mut self.packs {
            pack.id = pack_ids[&pack.id].clone();
            pack.category = format!("{namespace}/{}", pack.category);
            for reference in pack.references.values_mut() {
                if let Some(qualified) = pack_ids.get(&reference.template) {
                    reference.template.clone_from(qualified);
                }
            }
        }
        self.packs
            .sort_by(|left, right| (&left.category, &left.id).cmp(&(&right.category, &right.id)));
        Ok(self)
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

pub(crate) fn valid_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn valid_template_id(id: &str) -> bool {
    match id.split_once('/') {
        Some((namespace, local_id)) => {
            valid_identifier(namespace) && valid_identifier(local_id) && !local_id.contains('/')
        }
        None => valid_identifier(id),
    }
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
