//! Manages multiple A2UI surfaces.

use std::collections::HashMap;

use super::surface_model::SurfaceModel;
use crate::error::A2uiError;

/// Container for all active surfaces.
pub struct SurfaceGroupModel {
    surfaces: HashMap<String, SurfaceModel>,
}

impl Default for SurfaceGroupModel {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceGroupModel {
    pub fn new() -> Self {
        Self {
            surfaces: HashMap::new(),
        }
    }

    /// Add a surface. Returns error if surface ID already exists.
    pub fn add_surface(&mut self, surface: SurfaceModel) -> Result<(), A2uiError> {
        if self.surfaces.contains_key(&surface.id) {
            return Err(A2uiError::SurfaceExists(surface.id.clone()));
        }
        self.surfaces.insert(surface.id.clone(), surface);
        Ok(())
    }

    /// Get a surface by ID.
    pub fn get_surface(&self, id: &str) -> Option<&SurfaceModel> {
        self.surfaces.get(id)
    }

    /// Get a mutable surface by ID.
    pub fn get_surface_mut(&mut self, id: &str) -> Option<&mut SurfaceModel> {
        self.surfaces.get_mut(id)
    }

    /// Delete a surface by ID.
    pub fn delete_surface(&mut self, id: &str) -> Result<(), A2uiError> {
        self.surfaces
            .remove(id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(id.to_string()))?;
        Ok(())
    }

    /// Iterate over all surfaces.
    pub fn surfaces(&self) -> impl Iterator<Item = &SurfaceModel> {
        self.surfaces.values()
    }

    /// Iterate mutably over all surfaces.
    pub fn surfaces_mut(&mut self) -> impl Iterator<Item = &mut SurfaceModel> {
        self.surfaces.values_mut()
    }

    /// Number of active surfaces.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.surfaces.len()
    }

    /// `true` if there are no active surfaces.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }
}
