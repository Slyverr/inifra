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
    if selected.is_empty() {
        return Err(WorkflowError("select at least one template".into()));
    }
    let mut seen = BTreeSet::new();
    let mut packs = Vec::new();
    let mut used = BTreeMap::new();
    let mut directories = BTreeSet::new();
    for id in selected {
        if !seen.insert(id) {
            return Err(WorkflowError(format!("duplicate template {id:?}")));
        }
        let pack = catalog
            .packs
            .iter()
            .find(|pack| &pack.id == id)
            .ok_or_else(|| WorkflowError(format!("unknown template {id:?}")))?;
        for (name, definition) in catalog
            .input_definitions_for(pack)
            .map_err(|error| WorkflowError(error.to_string()))?
        {
            if let Some(existing) = used.get(&name)
                && existing != &definition
            {
                return Err(WorkflowError(format!(
                    "selected templates have conflicting definitions for input {name:?}"
                )));
            }
            used.insert(name, definition);
        }
        packs.push(pack);
    }
    let values = inputs::resolve(&used, supplied).map_err(|e| WorkflowError(e.to_string()))?;
    let resolved_outputs = catalog
        .resolve_outputs_for_packs(&packs)
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
        outputs.extend(
            render::render(&pack.directory, &sources, &values)
                .map_err(|e| WorkflowError(e.to_string()))?,
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
