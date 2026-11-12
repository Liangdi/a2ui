//! Recursive tree walker — the GPUI counterpart of the iced
//! `render_node` (`crates/iced/src/walker.rs`) and the egui `render_node`
//! (`crates/egui/src/walker.rs`).
//!
//! Like the egui walker there is no measure pass (gpui auto-layouts via taffy)
//! and no Slint-style flat-array/bounded-depth workaround (Rust recurses
//! natively). Unlike iced (which returns an owned `Element` from a stateless
//! `view`) GPUI rebuilds the element tree each frame **and** its stateful
//! widgets persist as `Entity<…State>` across frames — so this walker holds the
//! per-component-id state caches ([`text_states`] / [`slider_states`] /
//! [`select_states`]) behind `RefCell`s. That keeps [`Walk`] an immutable
//! `&Walk` borrow, which is what lets a [`ComponentContext`] (built from
//! `walk`'s `data_model` / `components` fields) coexist with the recursive
//! `&Walk` re-borrow for container children — the same borrow-split the egui
//! walker relies on, just with `RefCell` interior mutability standing in for
//! `&mut`.
//!
//! [`text_states`]: Walk::text_states
//! [`slider_states`]: Walk::slider_states
//! [`select_states`]: Walk::select_states

use a2ui_base::model::component_context::ComponentContext;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Window};

use crate::app::GpuiApp;
use crate::components::{
    Walk, render_button, render_card, render_checkbox, render_choice_picker, render_column,
    render_date_time_input, render_divider, render_icon, render_image, render_media_placeholder,
    render_modal, render_row, render_slider, render_tabs, render_text, render_text_field,
    render_unknown,
};

/// Recursively render a single A2UI component into a GPUI element tree.
///
/// Builds a [`ComponentContext`] for the node, then dispatches to the matching
/// `render_*` arm in [`crate::components`] by component type. Stateful
/// interactive widgets (TextField / Slider / ChoicePicker) lazily create their
/// persistent `Entity<…State>` on first render via the `RefCell` caches on
/// [`Walk`] and seed it from the data model.
pub(super) fn render_node(
    component_id: &str,
    base_path: &str,
    walk: &Walk,
    window: &mut Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let key = crate::reactive::instance_key(walk.surface_id, component_id, base_path);
    let Some(_guard) = RenderGuard::enter(&walk.render_path, key) else {
        return gpui_kit::div()
            .child("Component cycle or nesting limit reached")
            .into_any_element();
    };
    let comp_model = match walk.components.get(component_id) {
        Some(m) => m,
        None => {
            return gpui_kit::div()
                .child(format!("Component not found: {component_id}"))
                .into_any_element();
        }
    };

    if walk.current_protocol
        && let Err(error) = check_native_catalog(walk.processor, walk.catalog_id, comp_model)
    {
        return gpui_kit::div().child(error).into_any_element();
    }
    let reactive = walk.reactive.view(walk.surface_id, component_id, base_path);
    if walk.current_protocol && reactive.is_none() {
        return gpui_kit::div()
            .child("Component subscription limit reached")
            .into_any_element();
    }
    let mut overlay = comp_model.clone();
    if let Some(view) = reactive {
        if !view.errors.is_empty() {
            return gpui_kit::div()
                .text_color(gpui_kit::rgb(0xf38ba8))
                .child(view.errors.join("; "))
                .into_any_element();
        }
        if view.pending {
            return gpui_kit::div().child("Loading…").into_any_element();
        }
        overlay.properties.extend(view.resolved.clone());
        if overlay.properties.contains_key("@checks") {
            overlay.properties.insert(
                "@checks".into(),
                serde_json::json!([{"condition":view.checks_valid}]),
            );
        }
    }
    let comp_model = &overlay;
    let functions = if walk.current_protocol {
        walk.processor
            .catalog(walk.catalog_id)
            .map(|catalog| &catalog.functions)
            .unwrap_or(walk.empty_functions)
    } else {
        walk.functions
    };
    let mut ctx = ComponentContext::new(
        component_id.to_string(),
        walk.surface_id.to_string(),
        walk.data_model,
        walk.components,
        functions,
        base_path,
        walk.focused_id.map(str::to_string),
    );

    if walk.current_protocol {
        ctx.data_context = ctx
            .data_context
            .with_catalog_id(walk.catalog_id)
            .with_current_protocol();
    }

    let rendered = match comp_model.component_type.as_str() {
        // Containers.
        "Column" | "List" => render_column(walk, &ctx, comp_model, window, cx),
        "Row" => render_row(walk, &ctx, comp_model, window, cx),
        "Card" => render_card(walk, &ctx, comp_model, window, cx),
        "Tabs" => render_tabs(walk, &ctx, comp_model, window, cx),
        "Modal" => render_modal(walk, &ctx, comp_model, window, cx),

        // Content / leaf.
        "Text" => render_text(&ctx, comp_model),
        "Divider" => render_divider(),
        "Icon" => render_icon(&ctx, comp_model),
        "DateTimeInput" => render_date_time_input(walk, &ctx, comp_model, window, cx),
        "Image" => render_image(walk, &ctx, comp_model, cx),
        "Video" => render_media_placeholder("Video", &ctx, comp_model),
        "AudioPlayer" => render_media_placeholder("Audio", &ctx, comp_model),

        // Interactive (native GPUI Kit widgets).
        "Button" => render_button(walk, &ctx, comp_model, cx),
        "TextField" => render_text_field(walk, &ctx, comp_model, window, cx),
        "CheckBox" => render_checkbox(&ctx, comp_model, cx),
        "Slider" => render_slider(walk, &ctx, comp_model, window, cx),
        "ChoicePicker" => render_choice_picker(walk, &ctx, comp_model, window, cx),

        _ => render_unknown(walk, &ctx, comp_model, window, cx),
    };
    let rendered = crate::accessibility::wrap(&ctx, comp_model, rendered);
    if let Some(view) = reactive
        && (view.checks_pending || !view.check_messages.is_empty())
    {
        let mut wrapper = gpui_kit::div().flex().flex_col().gap_1().child(rendered);
        if view.checks_pending {
            wrapper = wrapper.child(
                gpui_kit::div()
                    .text_size(gpui_kit::px(12.))
                    .child("Checking…"),
            );
        }
        for message in &view.check_messages {
            wrapper = wrapper
                .border_1()
                .border_color(gpui_kit::rgb(0xf38ba8))
                .child(
                    gpui_kit::div()
                        .text_color(gpui_kit::rgb(0xf38ba8))
                        .text_size(gpui_kit::px(12.))
                        .child(message.clone()),
                );
        }
        wrapper.into_any_element()
    } else {
        rendered
    }
}

/// Built-in widget semantics are only valid for the catalogs that define them.
/// A custom catalog's same-named component must not acquire basic behavior.
pub(crate) fn check_native_catalog(
    processor: &a2ui_base::message_processor::MessageProcessor,
    surface_catalog: &str,
    component: &a2ui_base::model::component_model::ComponentModel,
) -> Result<(), String> {
    let id = component.catalog_id().unwrap_or(surface_catalog);
    let catalog = processor
        .catalog(id)
        .ok_or_else(|| format!("Unknown catalog: {id}"))?;
    if !matches!(
        id,
        "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
            | "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json"
    ) {
        return Err(format!("No GPUI Kit renderer registered for catalog: {id}"));
    }
    if catalog.get_component(&component.component_type).is_none() {
        return Err(format!(
            "Component {} is not in catalog {id}",
            component.component_type
        ));
    }
    Ok(())
}

struct RenderGuard<'a>(&'a std::cell::RefCell<Vec<String>>);
impl<'a> RenderGuard<'a> {
    fn enter(path: &'a std::cell::RefCell<Vec<String>>, key: String) -> Option<Self> {
        let mut active = path.borrow_mut();
        if active.len() >= 64 || active.contains(&key) {
            return None;
        }
        active.push(key);
        Some(Self(path))
    }
}
impl Drop for RenderGuard<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().pop();
    }
}
#[cfg(test)]
mod recursion_tests {
    use super::*;
    #[test]
    fn render_guard_rejects_cycles_and_deep_templates_and_releases_path() {
        let path = std::cell::RefCell::new(Vec::new());
        let root = RenderGuard::enter(&path, "root".into()).unwrap();
        assert!(RenderGuard::enter(&path, "root".into()).is_none());
        let mut guards = Vec::new();
        for index in 1..64 {
            guards.push(RenderGuard::enter(&path, format!("row/{index}")).unwrap());
        }
        assert!(RenderGuard::enter(&path, "too-deep".into()).is_none());
        while let Some(guard) = guards.pop() {
            drop(guard);
        }
        drop(root);
        assert!(path.borrow().is_empty());
        assert!(RenderGuard::enter(&path, "root".into()).is_some());
    }
}
