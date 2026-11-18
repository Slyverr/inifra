//! Coordinate catalog selection, input resolution, rendering, and planning.

use crate::{
    catalog::Pack,
    inputs::{self, InputDefinition},
    plan::{self, PlannedFile},
    render, sources,
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
    catalog: &[Pack],
    definitions: &BTreeMap<String, InputDefinition>,
    selected: &[String],
    supplied: &BTreeMap<String, String>,
    destination: &Path,
    overwrite: bool,
) -> Result<Vec<PlannedFile>, WorkflowError> {
    if selected.is_empty() {
        return Err(WorkflowError("select at least one template".into()));
    }
    let mut seen = BTreeSet::new();
    let mut packs = Vec::new();
    let mut used = BTreeMap::new();
    for id in selected {
        if !seen.insert(id) {
            return Err(WorkflowError(format!("duplicate template {id:?}")));
        }
        let pack = catalog
            .iter()
            .find(|pack| &pack.id == id)
            .ok_or_else(|| WorkflowError(format!("unknown template {id:?}")))?;
        for name in &pack.inputs {
            let definition = definitions.get(name).ok_or_else(|| {
                WorkflowError(format!("template {id:?} references unknown input {name:?}"))
            })?;
            used.insert(name.clone(), definition.clone());
        }
        packs.push(pack);
    }
    let values = inputs::resolve(&used, supplied).map_err(|e| WorkflowError(e.to_string()))?;
    let mut outputs = Vec::new();
    for pack in packs {
        let files = sources::discover(&pack.directory).map_err(|e| WorkflowError(e.to_string()))?;
        if files.is_empty() {
            return Err(WorkflowError(format!(
                "template {:?} has no sources",
                pack.id
            )));
        }
        outputs.extend(
            render::render(&pack.directory, &files, &values)
                .map_err(|e| WorkflowError(e.to_string()))?,
        );
    }
    plan::build(destination, outputs, overwrite).map_err(|e| WorkflowError(e.to_string()))
}
