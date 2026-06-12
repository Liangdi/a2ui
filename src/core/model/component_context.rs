//! Component rendering context — passed to component implementations during build.

use std::collections::HashMap;

use super::components_model::SurfaceComponentsModel;
use super::data_context::DataContext;
use super::data_model::DataModel;
use crate::core::catalog::function_api::FunctionImplementation;

/// Transient context created for each component during rendering.
///
/// The caller is responsible for holding the RefCell borrows on DataModel
/// and SurfaceComponentsModel for the duration of rendering.
pub struct ComponentContext<'a> {
    /// The component's ID.
    pub component_id: String,
    /// Scoped data access for resolving dynamic values.
    pub data_context: DataContext<'a>,
    /// The components model (escape hatch for inspecting siblings/children).
    pub components: &'a SurfaceComponentsModel,
}

impl<'a> ComponentContext<'a> {
    /// Create a component context.
    ///
    /// Callers should borrow `surface.data_model` and `surface.components`
    /// before calling this and pass the references.
    pub fn new(
        component_id: String,
        data_model: &'a DataModel,
        components: &'a SurfaceComponentsModel,
        functions: &'a HashMap<String, Box<dyn FunctionImplementation>>,
        base_path: &str,
    ) -> Self {
        let data_context = if base_path.is_empty() {
            DataContext::new(data_model, functions)
        } else {
            DataContext::new(data_model, functions).nested(base_path.trim_start_matches('/'))
        };

        Self {
            component_id,
            data_context,
            components,
        }
    }
}
