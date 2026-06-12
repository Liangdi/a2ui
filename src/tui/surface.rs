//! Surface renderer — entry point for rendering an A2UI component tree into a ratatui frame.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Paragraph},
};

use crate::core::catalog::function_api::FunctionImplementation;
use crate::core::catalog::Catalog;
use crate::core::model::component_context::ComponentContext;
use crate::core::model::components_model::SurfaceComponentsModel;
use crate::core::model::data_model::DataModel;
use crate::core::model::surface_model::SurfaceModel;
use super::component_impl::ComponentRegistry;

/// Renders a [`SurfaceModel`] into a ratatui frame by walking the component tree.
pub struct SurfaceRenderer<'a> {
    surface: &'a SurfaceModel,
    registry: &'a ComponentRegistry,
    catalog: &'a Catalog,
}

impl<'a> SurfaceRenderer<'a> {
    /// Create a new renderer for the given surface.
    pub fn new(
        surface: &'a SurfaceModel,
        registry: &'a ComponentRegistry,
        catalog: &'a Catalog,
    ) -> Self {
        Self {
            surface,
            registry,
            catalog,
        }
    }

    /// Main entry point: render the component tree into the frame.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let data_model = self.surface.data_model.borrow();
        let components = self.surface.components.borrow();

        // Look up the root component.
        if !components.contains("root") {
            let widget = Paragraph::new("No root component").block(Block::bordered());
            frame.render_widget(widget, area);
            return;
        }

        render_node(
            "root",
            "",
            area,
            frame,
            &data_model,
            &components,
            self.registry,
            &self.catalog.functions,
        );
    }

    /// Convenience method to render a child by ID with an explicit base path.
    ///
    /// Useful for template-based rendering where a container iterates over a
    /// data array and renders the same component for each item with a nested
    /// data path.
    pub fn render_child_by_id(
        &self,
        child_id: &str,
        base_path: &str,
        area: Rect,
        frame: &mut Frame,
        data_model: &DataModel,
        components: &SurfaceComponentsModel,
    ) {
        render_node(
            child_id,
            base_path,
            area,
            frame,
            data_model,
            components,
            self.registry,
            &self.catalog.functions,
        );
    }
}

/// Recursively render a single component node.
///
/// This free function is the core of the renderer. Each call:
/// 1. Looks up the component model by ID.
/// 2. Builds a [`ComponentContext`] for it.
/// 3. Finds the matching [`TuiComponent`](super::component_impl::TuiComponent) in the registry.
/// 4. Passes a `render_child` closure that re-enters this same function for any children.
fn render_node(
    component_id: &str,
    base_path: &str,
    area: Rect,
    frame: &mut Frame,
    data_model: &DataModel,
    components: &SurfaceComponentsModel,
    registry: &ComponentRegistry,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
) {
    let comp_model = match components.get(component_id) {
        Some(m) => m,
        None => {
            let msg = format!("Component not found: {}", component_id);
            let widget = Paragraph::new(msg).block(Block::bordered());
            frame.render_widget(widget, area);
            return;
        }
    };

    let ctx = ComponentContext::new(
        component_id.to_string(),
        data_model,
        components,
        functions,
        base_path,
    );

    let tui_comp = match registry.get(&comp_model.component_type) {
        Some(c) => c,
        None => {
            let msg = format!("Unknown component type: {}", comp_model.component_type);
            let widget = Paragraph::new(msg).block(Block::bordered());
            frame.render_widget(widget, area);
            return;
        }
    };

    // The render_child closure simply re-enters render_node for each child,
    // giving unbounded recursion depth without code duplication.
    let mut render_child = |child_id: &str, child_area: Rect, child_frame: &mut Frame| {
        render_node(
            child_id,
            base_path,
            child_area,
            child_frame,
            data_model,
            components,
            registry,
            functions,
        );
    };

    tui_comp.render(&ctx, area, frame, &mut render_child);
}
