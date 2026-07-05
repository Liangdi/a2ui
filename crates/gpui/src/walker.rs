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

use gpui::prelude::*;
use gpui::{AnyElement, Context, Window};

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
    let comp_model = match walk.components.get(component_id) {
        Some(m) => m,
        None => return gpui::div().child(format!("Component not found: {component_id}")).into_any_element(),
    };

    let ctx = ComponentContext::new(
        component_id.to_string(),
        walk.surface_id.to_string(),
        walk.data_model,
        walk.components,
        walk.functions,
        base_path,
        walk.focused_id.map(str::to_string),
    );

    match comp_model.component_type.as_str() {
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

        // Interactive (native gpui-component widgets).
        "Button" => render_button(&ctx, comp_model, cx),
        "TextField" => render_text_field(walk, &ctx, comp_model, window, cx),
        "CheckBox" => render_checkbox(&ctx, comp_model, cx),
        "Slider" => render_slider(walk, &ctx, comp_model, window, cx),
        "ChoicePicker" => render_choice_picker(walk, &ctx, comp_model, window, cx),

        _ => render_unknown(walk, &ctx, comp_model, window, cx),
    }
}
