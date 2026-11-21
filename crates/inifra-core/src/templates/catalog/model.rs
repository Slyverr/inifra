use std::{collections::BTreeMap, path::PathBuf};

use crate::{inputs::InputDefinition, output_rules::OutputRule};

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
