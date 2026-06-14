//! A single A2UI surface (a distinct UI region).

use std::cell::RefCell;
use std::collections::HashMap;

use super::components_model::SurfaceComponentsModel;
use super::data_model::DataModel;

/// Metadata for tracking actions awaiting a server response.
pub struct PendingAction {
    /// The unique action ID that was sent to the server.
    pub action_id: String,
    /// Optional JSON Pointer path where the response value should be stored.
    pub response_path: Option<String>,
}

/// State for a single A2UI surface.
pub struct SurfaceModel {
    /// Unique surface identifier.
    pub id: String,
    /// Catalog URI this surface uses.
    pub catalog_id: String,
    /// Optional surface properties (e.g. agentDisplayName).
    pub surface_properties: Option<serde_json::Value>,
    /// Whether to send the full data model with actions.
    pub send_data_model: bool,
    /// The data model for this surface.
    pub data_model: RefCell<DataModel>,
    /// The component tree for this surface.
    pub components: RefCell<SurfaceComponentsModel>,
    /// Actions that are awaiting a server response, keyed by action_id.
    pub pending_actions: RefCell<HashMap<String, PendingAction>>,
}

impl SurfaceModel {
    /// Create a new surface model.
    pub fn new(
        id: String,
        catalog_id: String,
        surface_properties: Option<serde_json::Value>,
        send_data_model: bool,
    ) -> Self {
        Self {
            id,
            catalog_id,
            surface_properties,
            send_data_model,
            data_model: RefCell::new(DataModel::new()),
            components: RefCell::new(SurfaceComponentsModel::new()),
            pending_actions: RefCell::new(HashMap::new()),
        }
    }

    /// Initialize with a data model value.
    pub fn with_data_model(mut self, data: serde_json::Value) -> Self {
        self.data_model = RefCell::new(DataModel::from_value(data));
        self
    }

    /// Check if the component tree has a root component.
    pub fn has_root(&self) -> bool {
        self.components.borrow().contains("root")
    }
}
