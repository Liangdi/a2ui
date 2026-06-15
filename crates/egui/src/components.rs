//! Per-component-kind egui render functions.
//!
//! Each `render_*` fn takes the pieces a component needs to render itself into
//! a `&mut egui::Ui` and (for interactive kinds) pushes any interaction onto a
//! `&mut Vec<PendingInteraction>` carried through the walk. Container fns
//! re-enter [`crate::walker::render_node`] for their children via the
//! `render_child` closure, mirroring how the ratatui `render_node` recurses.
//!
//! To keep egui's closure-based layout ergonomic without `unsafe`, the `&mut`
//! state (`ui`, `edit_buffers`, `pending`) is passed as separate function
//! arguments rather than bundled into one struct — so each closure receives a
//! fresh `&mut Ui` and re-borrows the other mut state cleanly.

use std::collections::{HashMap, HashSet};

use egui::Ui;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{ChildList, DynamicBoolean, DynamicNumber, DynamicString};

use crate::edit_state::EditBuffers;
use crate::interaction::PendingInteraction;
use crate::walker::render_node;

/// Shared read-only context threaded through every render function.
/// (`ui` / `edit_buffers` / `pending` are passed separately as `&mut`.)
pub(super) struct Walk<'a> {
    pub surface_id: &'a str,
    pub data_model: &'a DataModel,
    pub components: &'a SurfaceComponentsModel,
    pub functions: &'a HashMap<String, Box<dyn FunctionImplementation>>,
    pub focused_id: Option<&'a str>,
    pub open_modals: &'a HashSet<String>,
}

/// Re-enter the walker for one child.
#[allow(clippy::too_many_arguments)]
fn render_child(
    walk: &Walk<'_>,
    ui: &mut Ui,
    edit_buffers: &mut EditBuffers,
    pending: &mut Vec<PendingInteraction>,
    child_id: &str,
    base_path: &str,
) {
    render_node(
        child_id,
        walk.surface_id,
        base_path,
        ui,
        walk.data_model,
        walk.components,
        walk.functions,
        walk.focused_id,
        walk.open_modals,
        edit_buffers,
        pending,
    );
}

/// Plan a node's children as `(child_id, child_base_path)` pairs, honoring all
/// three A2UI child shapes (`child`, static `children`, template `children`).
///
/// Mirrors `crates/slint/src/live_tree.rs::build_child_plan`. Modal is handled
/// by its own renderer (trigger in-place; content as overlay), so it is excluded.
fn build_child_plan(model: &ComponentModel, ctx: &ComponentContext) -> Vec<(String, String)> {
    let mut plan = Vec::new();
    let base = ctx.data_context.base_path().to_string();

    if let Some(child_id) = model.child() {
        plan.push((child_id, base.clone()));
    }
    match model.children() {
        Some(ChildList::Static(ids)) => {
            for cid in ids {
                plan.push((cid.clone(), base.clone()));
            }
        }
        Some(ChildList::Template { component_id, path }) => {
            if let Some(serde_json::Value::Array(arr)) = ctx.data_context.get(&path) {
                for i in 0..arr.len() {
                    plan.push((component_id.clone(), format!("{path}/{i}")));
                }
            }
        }
        None => {}
    }
    plan
}

// ===========================================================================
// Containers
// ===========================================================================

/// Column / List — vertical stack of children.
pub(super) fn render_column(
    walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    ui.vertical(|ui| {
        for (child_id, child_base) in build_child_plan(model, ctx) {
            render_child(walk, ui, eb, p, &child_id, &child_base);
        }
    });
}

/// Row — horizontal stack of children.
pub(super) fn render_row(
    walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    ui.horizontal(|ui| {
        for (child_id, child_base) in build_child_plan(model, ctx) {
            render_child(walk, ui, eb, p, &child_id, &child_base);
        }
    });
}

/// Card — bordered panel wrapping its child.
pub(super) fn render_card(
    walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(208)))
        .corner_radius(8.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            for (child_id, child_base) in build_child_plan(model, ctx) {
                render_child(walk, ui, eb, p, &child_id, &child_base);
            }
        });
}

/// Modal — render its `trigger` child in-place. When open, the content floats
/// as a top-level `egui::Window` overlay (built by [`crate::app`] after the main
/// tree), so the trigger keeps its place and focus.
pub(super) fn render_modal(
    walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    _ctx: &ComponentContext, model: &ComponentModel,
) {
    if let Some(trigger_id) = model.get_property::<String>("trigger") {
        render_child(walk, ui, eb, p, &trigger_id, "");
    }
}

// ===========================================================================
// Content / leaf
// ===========================================================================

/// Text — styled label; `variant` h1/h2/h3 select heading sizes.
pub(super) fn render_text(ui: &mut Ui, ctx: &ComponentContext, model: &ComponentModel) {
    let text = model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let variant: Option<String> = model.get_property("variant");
    let richtext = if matches!(variant.as_deref(), Some("h1") | Some("h2") | Some("h3")) {
        egui::RichText::new(text).strong().heading()
    } else {
        egui::RichText::new(text)
    };
    ui.label(richtext);
}

/// Divider — thin horizontal rule.
pub(super) fn render_divider(ui: &mut Ui) {
    ui.separator();
}

// ===========================================================================
// Interactive
// ===========================================================================

/// Button — labeled press target. A click dispatches `Enter` to its component
/// via the core pipeline (reuses [`crate::dispatch_event`] +
/// [`crate::apply_event_result`] in `apply_pending`), like the Slint host's
/// `handle_activate`. The label is the Button's single `child` (a Text).
pub(super) fn render_button(
    walk: &Walk<'_>, ui: &mut Ui, _eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    let label = resolve_child_text(ctx, model).unwrap_or_else(|| {
        model
            .accessibility()
            .and_then(|a| a.label)
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
            .unwrap_or_default()
    });
    let variant: Option<String> = model.get_property("variant");
    let checks_pass = evaluate_checks(ctx, model);

    // egui's `Button` has no `underline()` setter; the borderless variant uses
    // an underlined label + no fill/stroke to read as a text link.
    let label_richtext = match variant.as_deref() {
        Some("borderless") => egui::RichText::new(label).underline(),
        other => {
            let rt = egui::RichText::new(label);
            match other {
                Some("primary") => rt.strong(),
                _ => rt,
            }
        }
    };
    let button = egui::Button::new(label_richtext);
    let button = match variant.as_deref() {
        Some("primary") => button
            .fill(egui::Color32::from_rgb(37, 99, 235))
            .stroke(egui::Stroke::NONE),
        Some("borderless") => button.fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE),
        _ => button,
    };
    let response = ui.add_enabled(checks_pass, button);
    if response.clicked() {
        p.push(PendingInteraction::ButtonActivate {
            component_id: ctx.component_id.clone(),
        });
    }
    let _ = walk;
}

/// TextField — egui native single-line edit, bridged to the data model.
pub(super) fn render_text_field(
    _walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value_binding = model.get_property::<DynamicString>("value");
    let resolved = value_binding
        .as_ref()
        .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
        .unwrap_or_default();

    if !label.is_empty() {
        ui.label(egui::RichText::new(&label).weak().small());
    }

    let focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());
    let buf = eb.text_buffer(&ctx.component_id, &resolved, focused);
    let before = buf.clone();
    ui.text_edit_singleline(buf);
    if buf != &before
        && let Some(DynamicString::Binding(b)) = &value_binding
    {
        p.push(PendingInteraction::DataUpdate {
            path: ctx.data_context.resolve_pointer(&b.path),
            value: serde_json::Value::String(buf.clone()),
        });
    }
}

/// CheckBox — egui native checkbox; toggles write back to the data model.
pub(super) fn render_checkbox(
    _walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value_binding = model.get_property::<DynamicBoolean>("value");
    let resolved = value_binding
        .as_ref()
        .map(|db| ctx.data_context.resolve_dynamic_boolean(db))
        .unwrap_or(false);

    let checked = eb.boolean_buffer(&ctx.component_id, resolved);
    let before = *checked;
    ui.checkbox(checked, &label);
    if *checked != before
        && let Some(DynamicBoolean::Binding(b)) = &value_binding
    {
        p.push(PendingInteraction::DataUpdate {
            path: ctx.data_context.resolve_pointer(&b.path),
            value: serde_json::Value::Bool(*checked),
        });
    }
}

/// Slider — egui native slider; value changes write back to the data model.
pub(super) fn render_slider(
    _walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    let value_binding = model.get_property::<DynamicNumber>("value");
    let resolved = value_binding
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn))
        .unwrap_or(0.0);
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    let val = eb.number_buffer(&ctx.component_id, resolved);
    let before = *val;
    ui.add(egui::Slider::new(val, 0.0..=100.0).text(&label));
    if (*val - before).abs() > f64::EPSILON
        && let Some(DynamicNumber::Binding(b)) = &value_binding
    {
        p.push(PendingInteraction::DataUpdate {
            path: ctx.data_context.resolve_pointer(&b.path),
            value: serde_json::json!(*val),
        });
    }
}

/// ChoicePicker — egui native ComboBox (P2 placeholder: label-only for now).
pub(super) fn render_choice_picker(_walk: &Walk<'_>, ui: &mut Ui, _eb: &mut EditBuffers, _p: &mut Vec<PendingInteraction>, ctx: &ComponentContext, model: &ComponentModel) {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    ui.label(format!("[ChoicePicker: {label}]"));
}

/// Tabs — render the active child panel (P2 adds the tab bar).
pub(super) fn render_tabs(
    walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    let active = model
        .get_property::<DynamicNumber>("activeTab")
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn))
        .unwrap_or(0.0) as usize;
    let plan = build_child_plan(model, ctx);
    if let Some((child_id, child_base)) = plan.get(active) {
        render_child(walk, ui, eb, p, child_id, child_base);
    }
}

/// Icon — labeled box (no icon font); placeholder.
pub(super) fn render_icon(ui: &mut Ui, ctx: &ComponentContext, model: &ComponentModel) {
    let name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    ui.label(format!("[icon: {name}]"));
}

/// DateTimeInput — bordered field showing label + value (P2 full impl).
pub(super) fn render_date_time_input(ui: &mut Ui, ctx: &ComponentContext, model: &ComponentModel) {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value = model
        .get_property::<DynamicString>("value")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    ui.label(format!("{label}: {value}"));
}

/// Image / Video — labeled placeholder (real texture loading in P3).
pub(super) fn render_media_placeholder(ui: &mut Ui, kind: &str, ctx: &ComponentContext, model: &ComponentModel) {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    ui.label(format!("[{kind}: {url}]"));
}

/// Unknown / not-yet-implemented kind — show the kind name + recurse children.
pub(super) fn render_unknown(
    walk: &Walk<'_>, ui: &mut Ui, eb: &mut EditBuffers, p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext, model: &ComponentModel,
) {
    ui.label(format!("[{}]", model.component_type));
    for (child_id, child_base) in build_child_plan(model, ctx) {
        render_child(walk, ui, eb, p, &child_id, &child_base);
    }
}

// ===========================================================================
// Field helpers
// ===========================================================================

/// Resolve a Button's child Text label (if its `child` is a Text component).
fn resolve_child_text(ctx: &ComponentContext, model: &ComponentModel) -> Option<String> {
    let child_id = model.child()?;
    let child = ctx.components.get(&child_id)?;
    if child.component_type != "Text" {
        return None;
    }
    child
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
}

/// Evaluate all `checks` on the component. Returns `true` if all pass (or none).
fn evaluate_checks(ctx: &ComponentContext, model: &ComponentModel) -> bool {
    match model.checks() {
        Some(checks) => checks
            .iter()
            .all(|rule| ctx.data_context.resolve_dynamic_boolean_condition(&rule.condition)),
        None => true,
    }
}
