//! Recursive tree walker — the egui counterpart of the ratatui
//! `render_node` (`crates/tui/src/surface.rs`).
//!
//! Unlike the ratatui walker there is no measure pass (egui auto-layouts), and
//! unlike the Slint backend there is no flat-array/bounded-depth workaround
//! (Rust recurses natively). Each call builds a [`ComponentContext`], dispatches
//! to the matching `render_*` arm in [`crate::components`] by component type,
//! and accumulates interactions into `pending`.

use std::collections::{HashMap, HashSet};

use egui::Ui;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;

use crate::components::{Walk, render_button, render_card, render_checkbox, render_choice_picker,
    render_column, render_date_time_input, render_divider, render_icon, render_media_placeholder,
    render_modal, render_row, render_slider, render_tabs, render_text, render_text_field,
    render_unknown};
use crate::edit_state::EditBuffers;
use crate::interaction::PendingInteraction;

/// Recursively render a single A2UI component into an egui `Ui`.
#[allow(clippy::too_many_arguments)]
pub fn render_node(
    component_id: &str,
    surface_id: &str,
    base_path: &str,
    ui: &mut Ui,
    data_model: &DataModel,
    components: &SurfaceComponentsModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
    open_modals: &HashSet<String>,
    edit_buffers: &mut EditBuffers,
    pending: &mut Vec<PendingInteraction>,
) {
    let comp_model = match components.get(component_id) {
        Some(m) => m,
        None => {
            ui.label(format!("Component not found: {component_id}"));
            return;
        }
    };

    let walk = Walk {
        surface_id,
        data_model,
        components,
        functions,
        focused_id,
        open_modals,
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
        "Column" | "List" => render_column(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "Row" => render_row(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "Card" => render_card(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "Tabs" => render_tabs(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "Modal" => render_modal(&walk, ui, edit_buffers, pending, &ctx, comp_model),

        // Content / leaf.
        "Text" => render_text(ui, &ctx, comp_model),
        "Divider" => render_divider(ui),
        "Icon" => render_icon(ui, &ctx, comp_model),
        "DateTimeInput" => render_date_time_input(ui, &ctx, comp_model),
        "Image" => render_media_placeholder(ui, "Image", &ctx, comp_model),
        "Video" => render_media_placeholder(ui, "Video", &ctx, comp_model),
        "AudioPlayer" => render_media_placeholder(ui, "Audio", &ctx, comp_model),

        // Interactive (native egui widgets).
        "Button" => render_button(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "TextField" => render_text_field(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "CheckBox" => render_checkbox(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "Slider" => render_slider(&walk, ui, edit_buffers, pending, &ctx, comp_model),
        "ChoicePicker" => render_choice_picker(&walk, ui, edit_buffers, pending, &ctx, comp_model),

        _ => render_unknown(&walk, ui, edit_buffers, pending, &ctx, comp_model),
    }
}
