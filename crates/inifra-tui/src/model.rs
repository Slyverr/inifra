use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

/// Catalog information displayed by the initialization picker.
#[derive(Clone, Debug)]
pub struct CatalogView {
    /// Categories in their display order.
    pub categories: Vec<CategoryView>,
    /// Available template choices in their display order.
    pub templates: Vec<TemplateChoice>,
}

/// A template category.
#[derive(Clone, Debug)]
pub struct CategoryView {
    /// Stable category identifier.
    pub id: String,
    /// Human-readable category name.
    pub name: String,
}

/// One picker row containing mutually exclusive templates with shared outputs.
#[derive(Clone, Debug)]
pub struct TemplateChoice {
    /// Identifier of the category containing this choice.
    pub category_id: String,
    /// Templates available by cycling left or right. Single-template choices have one variant.
    pub variants: Vec<TemplateView>,
}

/// A template's information for the picker and preview.
#[derive(Clone, Debug)]
pub struct TemplateView {
    /// Stable template identifier.
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Shared and pack-local input names and prompts shown in the preview.
    pub inputs: Vec<TemplateInputView>,
    /// Intentionally empty output directories.
    pub directories: Vec<PathBuf>,
    /// Source contents and output rules shown in the preview.
    pub sources: Vec<TemplateSourceView>,
}

/// Final output paths resolved for the current picker selection.
#[derive(Clone, Debug, Default)]
pub struct SelectionPreview {
    /// Final output paths grouped by template identifier and source path.
    pub outputs: BTreeMap<String, BTreeMap<PathBuf, PathBuf>>,
    /// IDs of templates that cannot be selected with the current selection.
    pub unavailable: BTreeSet<String>,
}

/// A template input shown in the template preview.
#[derive(Clone, Debug)]
pub struct TemplateInputView {
    /// Stable input name.
    pub name: String,
    /// Human-readable input prompt.
    pub prompt: String,
}

/// One source file shown in the template preview.
#[derive(Clone, Debug)]
pub struct TemplateSourceView {
    /// Path to the source file relative to the template pack.
    pub source: PathBuf,
    /// Default output path for this source.
    pub output: PathBuf,
    /// Alternate output used when a compatible template is selected with this one.
    pub alternate_output: Option<PathBuf>,
    /// Source text, or a safe description for non-text sources.
    pub content: String,
}

impl TemplateSourceView {
    pub(crate) fn resolved_output<'a>(
        &'a self,
        outputs: Option<&'a BTreeMap<PathBuf, PathBuf>>,
    ) -> &'a std::path::Path {
        outputs
            .and_then(|outputs| outputs.get(&self.source))
            .unwrap_or(&self.output)
    }
}

/// A value to request once for the selected templates.
#[derive(Clone, Debug)]
pub struct InputPrompt {
    /// Input identifier.
    pub name: String,
    /// Human-readable prompt.
    pub prompt: String,
    /// Whether an empty value is invalid.
    pub required: bool,
    /// Optional value used when the user leaves the field empty.
    pub default: Option<String>,
}

/// Operation shown for a planned output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewAction {
    /// Create an output that does not yet exist.
    Create,
    /// Replace an existing file.
    Replace,
    /// Leave an existing output unchanged.
    Skip,
}

impl ReviewAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Replace => "replace",
            Self::Skip => "keep",
        }
    }
}

/// A planned file operation displayed before approval.
#[derive(Clone, Debug)]
pub struct ReviewedFile {
    /// Destination relative to the target.
    pub path: PathBuf,
    /// Human-readable operation.
    pub action: ReviewAction,
}

/// A planned directory operation displayed before approval.
#[derive(Clone, Debug)]
pub struct ReviewedDirectory {
    /// Destination relative to the target.
    pub path: PathBuf,
    /// Human-readable operation.
    pub action: ReviewAction,
}

/// Destination and operations for the prepared plan.
#[derive(Clone, Debug)]
pub struct GenerationReview {
    /// Destination directory.
    pub target: String,
    /// Planned file operations.
    pub files: Vec<ReviewedFile>,
    /// Planned directory operations.
    pub directories: Vec<ReviewedDirectory>,
}

/// Application services used by the interactive presentation.
///
/// Implementations own validation and writing. The UI passes the exact plan
/// returned by `prepare` to `apply` only after the user approves it.
pub trait InitBackend {
    /// Opaque validated plan retained until approval.
    type Plan;

    /// Resolves the inputs needed for a selection.
    ///
    /// # Errors
    /// Returns a user-facing explanation for an invalid selection.
    fn input_prompts(&mut self, selected: &[String]) -> Result<Vec<InputPrompt>, String>;
    /// Validates a candidate selection before the picker accepts it.
    ///
    /// # Errors
    /// Returns a user-facing explanation when selected templates cannot be combined.
    fn validate_selection(&mut self, _selected: &[String]) -> Result<(), String> {
        Ok(())
    }
    /// Resolves display paths and reports templates incompatible with the current selection.
    ///
    /// The default implementation validates without overriding paths or marking templates unavailable.
    ///
    /// # Errors
    /// Returns a user-facing explanation when the selection cannot be combined.
    fn selection_preview(&mut self, selected: &[String]) -> Result<SelectionPreview, String> {
        self.validate_selection(selected)?;
        Ok(SelectionPreview::default())
    }
    /// Prepares the operations without applying them.
    ///
    /// # Errors
    /// Returns a user-facing explanation if the plan cannot be prepared.
    fn prepare(
        &mut self,
        selected: &[String],
        values: &BTreeMap<String, String>,
    ) -> Result<(GenerationReview, Self::Plan), String>;
    /// Applies the exact approved plan.
    ///
    /// # Errors
    /// Returns a user-facing explanation if applying the plan fails.
    fn apply(&mut self, plan: Self::Plan) -> Result<(), String>;
}

/// Result of the interactive initialization.
#[derive(Clone, Debug)]
pub enum InitOutcome {
    /// The user approved and the backend applied the plan described here.
    Applied(GenerationReview),
    /// The user left without applying a plan.
    Cancelled,
}
