//! Ratatui-specific component trait and registry.
//!
//! Each A2UI component type (Text, Button, etc.) implements [`TuiComponent`]
//! so the renderer can delegate rendering to the appropriate handler.

use std::collections::HashMap;

use ratatui::{Frame, layout::Rect};

use a2ui_core::model::component_context::ComponentContext;

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
    /// - `measure_child` is a closure to ask a child for its natural content height
    ///   given an available width, mirroring `render_child`'s `(id, base_path, …)`
    ///   shape so template children measure against their own data path.
    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    );

    /// The intrinsic content height of this component **including its own chrome**
    /// (margins/borders), given `available_width` cells.
    ///
    /// `measure_child` lets container components measure their own children to sum
    /// (Column/vertical-List) or max (Row) their natural heights. Leaf components
    /// ignore it.
    ///
    /// Returning `None` means "no opinion" — containers treat the component as a
    /// legacy fill participant (it gets an equal/weighted share of the available
    /// space, exactly as before this measure pass existed). Leaf/content components
    /// override this to return a content-driven height so containers can reserve
    /// only as much vertical space as the content actually needs.
    ///
    /// The default `None` keeps unconverted components behaving exactly as today,
    /// so migration is gradual and regression-free.
    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        None
    }

    /// Handle an input event directed at this component.
    ///
    /// Returns `Some(EventResult)` if the component produced an action or data change
    /// that the application should process, or `None` if the event was not handled.
    ///
    /// The default implementation ignores all events (non-interactive components).
    fn handle_event(
        &self,
        _ctx: &ComponentContext,
        _event: &a2ui_core::event::InputEvent,
    ) -> Option<a2ui_core::event::EventResult> {
        None
    }
}

// After the workspace split, ComponentApi lives in a2ui-core — an external
// crate from here — so the blanket `impl<T: TuiComponent> ComponentApi for T`
// would violate the orphan rule (foreign trait for a bare type parameter).
// Instead we impl ComponentApi concretely for each registered component type
// (a local type impl'ing a foreign trait is always allowed). Add a line here
// whenever a new component is registered into a catalog.
macro_rules! impl_component_api {
    ($t:path) => {
        impl a2ui_core::catalog::component_api::ComponentApi for $t {
            fn name(&self) -> &'static str {
                <Self as crate::component_impl::TuiComponent>::name(self)
            }
        }
    };
}

impl_component_api!(crate::components::audio_player::AudioPlayerComponent);
impl_component_api!(crate::components::button::ButtonComponent);
impl_component_api!(crate::components::card::CardComponent);
impl_component_api!(crate::components::checkbox::CheckBoxComponent);
impl_component_api!(crate::components::choice_picker::ChoicePickerComponent);
impl_component_api!(crate::components::column::ColumnComponent);
impl_component_api!(crate::components::date_time_input::DateTimeInputComponent);
impl_component_api!(crate::components::divider::DividerComponent);
impl_component_api!(crate::components::icon::IconComponent);
impl_component_api!(crate::components::image::ImageComponent);
impl_component_api!(crate::components::list::ListComponent);
impl_component_api!(crate::components::modal::ModalComponent);
impl_component_api!(crate::components::row::RowComponent);
impl_component_api!(crate::components::slider::SliderComponent);
impl_component_api!(crate::components::tabs::TabsComponent);
impl_component_api!(crate::components::text::TextComponent);
impl_component_api!(crate::components::text_field::TextFieldComponent);
impl_component_api!(crate::components::video::VideoComponent);

/// Registry that maps component type names to their [`TuiComponent`] implementations.
pub type ComponentRegistry = HashMap<String, Box<dyn TuiComponent>>;

/// Build a [`ComponentRegistry`] from a list of component implementations.
///
/// Each component is keyed by its [`TuiComponent::name`].
///
/// # Example
///
/// ```ignore
/// use crate::component_impl::{ComponentRegistry, build_registry};
/// use crate::components::text::TextComponent;
/// use crate::components::button::ButtonComponent;
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
            _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
        ) {
        }
    }

    #[test]
    fn build_registry_keys_by_name() {
        let registry = build_registry(vec![Box::new(FakeComponent)]);
        assert!(registry.contains_key("Fake"));
    }
}
