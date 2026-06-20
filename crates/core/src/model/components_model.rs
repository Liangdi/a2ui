//! Flat map of ComponentModels for a single surface.

use std::collections::HashMap;

use super::component_model::ComponentModel;
use crate::error::A2uiError;

/// Manages all components in a surface as a flat HashMap.
pub struct SurfaceComponentsModel {
    components: HashMap<String, ComponentModel>,
}

impl Default for SurfaceComponentsModel {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceComponentsModel {
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
        }
    }

    /// Get a component by ID.
    pub fn get(&self, id: &str) -> Option<&ComponentModel> {
        self.components.get(id)
    }

    /// Get a mutable component by ID.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut ComponentModel> {
        self.components.get_mut(id)
    }

    /// Add or update a component.
    /// If the component already exists with a different type, replaces it.
    pub fn upsert(&mut self, component: ComponentModel) {
        self.components.insert(component.id.clone(), component);
    }

    /// Remove a component by ID.
    pub fn remove(&mut self, id: &str) {
        self.components.remove(id);
    }

    /// Returns true if a component with the given ID exists.
    pub fn contains(&self, id: &str) -> bool {
        self.components.contains_key(id)
    }

    /// Get all components.
    pub fn all(&self) -> &HashMap<String, ComponentModel> {
        &self.components
    }

    /// Parse and add multiple components from raw JSON.
    pub fn add_from_json(
        &mut self,
        raw_components: &[serde_json::Value],
    ) -> Vec<Result<(), A2uiError>> {
        raw_components
            .iter()
            .map(|raw| {
                let model = ComponentModel::from_json(raw)?;
                self.upsert(model);
                Ok(())
            })
            .collect()
    }

    /// Returns the number of components.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Returns true if there are no components.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}
