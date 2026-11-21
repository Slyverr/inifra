use super::{Catalog, CatalogError, valid_identifier};
use std::collections::BTreeMap;

impl Catalog {
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
}
