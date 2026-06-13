//! Ratatui-specific component trait and registry.
//!
//! Each A2UI component type (Text, Button, etc.) implements [`TuiComponent`]
//! so the renderer can delegate rendering to the appropriate handler.

use std::collections::HashMap;

use ratatui::{Frame, layout::Rect};

use crate::core::model::component_context::ComponentContext;

/// Trait for ratatui component implementations.
///
/// Each A2UI component type (Text, Button, etc.) implements this.
pub trait TuiComponent: Send + Sync + 'static {
    /// The component name (must match the A2UI catalog name).
    fn name(&self) -> &'static str;

    /// Render this component.
    ///
    /// - `ctx` provides access to the component's properties and data bindings.
    /// - `area` is the allocated area for this component.
    /// - `frame` is the ratatui frame to render into.
    /// - `render_child` is a closure to recursively render a child component by ID.
    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
    );

    /// Handle an input event directed at this component.
    ///
    /// Returns `Some(EventResult)` if the component produced an action or data change
    /// that the application should process, or `None` if the event was not handled.
    ///
    /// The default implementation ignores all events (non-interactive components).
    fn handle_event(
        &self,
        _ctx: &ComponentContext,
        _event: &crate::core::event::InputEvent,
    ) -> Option<crate::core::event::EventResult> {
        None
    }
}

// Blanket impl: any TuiComponent automatically satisfies ComponentApi.
// This must live in the tui layer (not core) because it depends on both traits.
impl<T: TuiComponent> crate::core::catalog::component_api::ComponentApi for T {
    fn name(&self) -> &'static str {
        <Self as TuiComponent>::name(self)
    }
}

/// Registry that maps component type names to their [`TuiComponent`] implementations.
pub type ComponentRegistry = HashMap<String, Box<dyn TuiComponent>>;

/// Build a [`ComponentRegistry`] from a list of component implementations.
///
/// Each component is keyed by its [`TuiComponent::name`].
///
/// # Example
///
/// ```ignore
/// use crate::tui::component_impl::{ComponentRegistry, build_registry};
/// use crate::tui::components::text::TextComponent;
/// use crate::tui::components::button::ButtonComponent;
///
/// let registry = build_registry(vec![
///     Box::new(TextComponent),
///     Box::new(ButtonComponent),
/// ]);
/// ```
pub fn build_registry(components: Vec<Box<dyn TuiComponent>>) -> ComponentRegistry {
    components
        .into_iter()
        .map(|c| {
            let name = c.name().to_string();
            (name, c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial component for testing the registry.
    struct FakeComponent;

    impl TuiComponent for FakeComponent {
        fn name(&self) -> &'static str {
            "Fake"
        }

        fn render(
            &self,
            _ctx: &ComponentContext,
            _area: Rect,
            _frame: &mut Frame,
            _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        ) {
        }
    }

    #[test]
    fn build_registry_keys_by_name() {
        let registry = build_registry(vec![Box::new(FakeComponent)]);
        assert!(registry.contains_key("Fake"));
    }
}
