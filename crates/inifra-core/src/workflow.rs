//! Coordinate catalog selection, input resolution, rendering, and planning.

use crate::{
    catalog::Catalog,
    inputs,
    plan::{self, GenerationPlan},
    render,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::Path,
};

/// A selection cannot be prepared for generation.
#[derive(Debug)]
pub struct WorkflowError(String);
impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for WorkflowError {}

/// Returns the effective input definitions for a selected set of templates.
///
/// Shared definitions are resolved using the catalog's scope precedence. Definitions with the
/// same name must be identical across every selected template so an interactive client can ask
/// for each value once.
///
/// # Errors
/// Returns an error for an empty or invalid selection, repeated IDs, unknown templates, or
/// conflicting definitions for the same input.
pub fn input_definitions_for_selection(
    catalog: &Catalog,
    selected: &[String],
) -> Result<BTreeMap<String, crate::inputs::InputDefinition>, WorkflowError> {
    let packs = selected_packs(catalog, selected)?;
    input_definitions_for_packs(catalog, &packs)
}

/// Prepares a generation plan for the selected pack IDs without writing files.
///
/// # Errors
/// Returns an error for unknown or repeated IDs, undeclared inputs, missing required
/// values, source or rendering failures, and invalid output destinations.
pub fn prepare(
    catalog: &Catalog,
    selected: &[String],
    supplied: &BTreeMap<String, String>,
    destination: &Path,
    overwrite: bool,
) -> Result<GenerationPlan, WorkflowError> {
    let packs = selected_packs(catalog, selected)?;
    let used = input_definitions_for_packs(catalog, &packs)?;
    let mut directories = BTreeSet::new();
    let values = inputs::resolve(&used, supplied).map_err(|e| WorkflowError(e.to_string()))?;
    let resolved_outputs = catalog
        .resolve_outputs_for_packs(&packs)
        .map_err(|error| WorkflowError(error.to_string()))?;
    let references = catalog
        .resolve_references_for_packs(&packs, &resolved_outputs)
        .map_err(|error| WorkflowError(error.to_string()))?;
    let mut outputs = Vec::new();
    for pack in packs {
        directories.extend(pack.directories.iter().cloned());
        let sources = resolved_outputs
            .iter()
            .filter(|output| output.template_id == pack.id)
            .map(|output| {
                let mut source = output.source.clone();
                source.output.clone_from(&output.output);
                source
            })
            .collect::<Vec<_>>();
        let pack_references = references.get(&pack.id).ok_or_else(|| {
            WorkflowError(format!("missing resolved references for {:?}", pack.id))
        })?;
        outputs.extend(
            render::render_with_references(&pack.directory, &sources, &values, pack_references)
                .map_err(|error| WorkflowError(error.to_string()))?,
        );
    }
    plan::build_with_directories(
        destination,
        outputs,
        directories.into_iter().collect(),
        overwrite,
    )
    .map_err(|e| WorkflowError(e.to_string()))
}

fn selected_packs<'a>(
    catalog: &'a Catalog,
    selected: &[String],
) -> Result<Vec<&'a crate::catalog::Pack>, WorkflowError> {
    if selected.is_empty() {
        return Err(WorkflowError("select at least one template".into()));
    }
    let mut seen = BTreeSet::new();
    selected
        .iter()
        .map(|id| {
            if !seen.insert(id) {
                return Err(WorkflowError(format!("duplicate template {id:?}")));
            }
            catalog
                .packs
                .iter()
                .find(|pack| &pack.id == id)
                .ok_or_else(|| WorkflowError(format!("unknown template {id:?}")))
        })
        .collect()
}

fn input_definitions_for_packs(
    catalog: &Catalog,
    packs: &[&crate::catalog::Pack],
) -> Result<BTreeMap<String, crate::inputs::InputDefinition>, WorkflowError> {
    let mut definitions = BTreeMap::new();
    for pack in packs {
        for (name, definition) in catalog
            .input_definitions_for(pack)
            .map_err(|error| WorkflowError(error.to_string()))?
        {
            if let Some(existing) = definitions.get(&name)
                && existing != &definition
            {
                return Err(WorkflowError(format!(
                    "selected templates have conflicting definitions for input {name:?}"
                )));
            }
            definitions.insert(name, definition);
        }
    }
    Ok(definitions)
}
