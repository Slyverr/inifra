use super::{Catalog, CatalogError, Pack};
use crate::{
    inputs::InputDefinition,
    output_rules::{self, ResolvedPackOutput},
    plan, render, sources,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

impl Catalog {
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
