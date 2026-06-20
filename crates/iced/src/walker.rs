//! Recursive tree walker — the Iced counterpart of the egui
//! `render_node` (`crates/egui/src/walker.rs`) and the ratatui
//! `render_node` (`crates/tui/src/surface.rs`).
//!
//! Like the egui walker there is no measure pass (iced auto-layouts) and no
//! Slint-style flat-array/bounded-depth workaround (Rust recurses natively).
//! Unlike egui this is **pure** — it builds and returns an owned [`Element`]
//! tree (no `&mut Ui`, no collected `pending` vec). Interactions are encoded as
//! [`Message`](crate::Message)s attached to the widgets, which
//! [`IcedApp::update`](crate::IcedApp) handles after the view returns. Each
//! call builds a [`ComponentContext`], dispatches to the matching `render_*`
//! arm in [`crate::components`] by component type.

use std::collections::HashMap;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;

use iced::Element;
use iced::widget::image;

use crate::components::{
    Walk, render_button, render_card, render_checkbox, render_choice_picker, render_column,
    render_date_time_input, render_divider, render_icon, render_image, render_media_placeholder,
    render_modal, render_row, render_slider, render_tabs, render_text, render_text_field,
    render_unknown,
};
use crate::message::Message;

/// Recursively render a single A2UI component into an Iced [`Element`] tree.
///
/// `image_cache` is the app-wide remote-image cache (see [`crate::IcedApp`]):
/// resolved `http(s)` URLs → their decoded [`image::Handle`] once the
/// background fetch completes. Local-file images are decoded directly and do
/// not consult the cache.
///
/// The returned element owns all of its content (see the lifetime note in
/// [`crate::components`]); `'a` is not tied to any input borrow.
#[allow(clippy::too_many_arguments)]
pub fn render_node<'a>(
    component_id: &str,
    surface_id: &str,
    base_path: &str,
    data_model: &DataModel,
    components: &SurfaceComponentsModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
    image_cache: &HashMap<String, Option<image::Handle>>,
    local_tabs: &HashMap<String, usize>,
) -> Element<'a, Message> {
    let comp_model = match components.get(component_id) {
        Some(m) => m,
        None => return iced::widget::text(format!("Component not found: {component_id}")).into(),
    };

    let walk = Walk {
        surface_id,
        data_model,
        components,
        functions,
        focused_id,
        image_cache,
        local_tabs,
    };

    let ctx = ComponentContext::new(
        component_id.to_string(),
        surface_id.to_string(),
        data_model,
        components,
        functions,
        base_path,
        focused_id.map(|s| s.to_string()),
    );

    match comp_model.component_type.as_str() {
        // Containers.
        "Column" | "List" => render_column(&walk, &ctx, comp_model),
        "Row" => render_row(&walk, &ctx, comp_model),
        "Card" => render_card(&walk, &ctx, comp_model),
        "Tabs" => render_tabs(&walk, &ctx, comp_model),
        "Modal" => render_modal(&walk, &ctx, comp_model),

        // Content / leaf.
        "Text" => render_text(&ctx, comp_model),
        "Divider" => render_divider(),
        "Icon" => render_icon(&ctx, comp_model),
        "DateTimeInput" => render_date_time_input(&ctx, comp_model),
        "Image" => render_image(&walk, &ctx, comp_model),
        "Video" => render_media_placeholder("Video", &ctx, comp_model),
        "AudioPlayer" => render_media_placeholder("Audio", &ctx, comp_model),

        // Interactive (native Iced widgets).
        "Button" => render_button(&ctx, comp_model),
        "TextField" => render_text_field(&ctx, comp_model),
        "CheckBox" => render_checkbox(&ctx, comp_model),
        "Slider" => render_slider(&ctx, comp_model),
        "ChoicePicker" => render_choice_picker(&ctx, comp_model),

        _ => render_unknown(&walk, &ctx, comp_model),
    }
}
