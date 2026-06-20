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

use egui::{TextureHandle, Ui};

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{
    ChildList, DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList, DynamicValue,
};
use serde_json::Value;

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
    /// Image cache: a resolved URL → `Some(decoded TextureHandle)` once decoded,
    /// or `None` for a URL that was attempted but failed to fetch/decode (so it
    /// isn't retried every frame). Decoded once on the UI thread by
    /// `EguiApp::load_images`. Mirrors the Iced/Bevy image caches.
    pub image_cache: &'a HashMap<String, Option<TextureHandle>>,
    /// Locally-tracked active tab index for Tabs components whose `activeTab`
    /// is **not** a data binding (the gallery samples fall here). Keyed by
    /// component id. Bound Tabs write to the model instead and don't use this.
    pub local_tabs: &'a HashMap<String, usize>,
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
        walk.image_cache,
        walk.local_tabs,
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
    walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
) {
    ui.vertical(|ui| {
        for (child_id, child_base) in build_child_plan(model, ctx) {
            render_child(walk, ui, eb, p, &child_id, &child_base);
        }
    });
}

/// Row — horizontal stack of children.
pub(super) fn render_row(
    walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
) {
    ui.horizontal(|ui| {
        for (child_id, child_base) in build_child_plan(model, ctx) {
            render_child(walk, ui, eb, p, &child_id, &child_base);
        }
    });
}

/// Card — bordered panel wrapping its child.
pub(super) fn render_card(
    walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
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
    walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    _ctx: &ComponentContext,
    model: &ComponentModel,
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
    walk: &Walk<'_>,
    ui: &mut Ui,
    _eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
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
        Some("borderless") => button
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE),
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
    _walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
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
    _walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
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
    _walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
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

/// ChoicePicker — a list of selectable options bridged to the data model.
///
/// - Single selection (`variant != "multipleSelection"`) renders a native
///   [`egui::ComboBox`] dropdown; choosing an option writes back `json!([value])`
///   (an array, matching the TUI backend's `EventResult` and the Iced backend).
/// - Multiple selection (`variant == "multipleSelection"`) renders a column of
///   native checkboxes; toggling adds/removes the value in the array written
///   back.
///
/// Only a `Binding` `value` is writable; a `Literal`/`Function`/absent value
/// renders read-only (single: a no-op dropdown; multi: inert checkboxes),
/// matching how the TUI `handle_event` bails on non-binding values. egui derives
/// the selection from the data model each frame, so — unlike TextField — no
/// `EditBuffers` slot is needed.
pub(super) fn render_choice_picker(
    _walk: &Walk<'_>,
    ui: &mut Ui,
    _eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
) {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let options = read_options(model);

    let value_binding = model.get_property::<DynamicStringList>("value");
    let selected_values = value_binding
        .as_ref()
        .map(|dsl| resolve_choice_value(ctx, dsl))
        .unwrap_or_default();

    // Only a Binding is writable; resolve its absolute write-back path.
    let path: Option<String> = match &value_binding {
        Some(DynamicStringList::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };

    if !label.is_empty() {
        ui.label(egui::RichText::new(&label).weak().small());
    }
    if options.is_empty() {
        return;
    }

    let is_multiple = model
        .get_property::<String>("variant")
        .as_deref()
        .map(|v| v == "multipleSelection")
        .unwrap_or(false);

    if is_multiple {
        // Multiple selection — a column of native checkboxes. Each toggle
        // recomputes the selection array from the value captured at render
        // time (render runs fresh each frame, so the captured set is current).
        ui.vertical(|ui| {
            for (opt_label, opt_value) in &options {
                let mut checked = selected_values.contains(opt_value);
                let response = ui.checkbox(&mut checked, opt_label);
                if response.changed()
                    && let Some(path) = &path
                {
                    let mut next = selected_values.clone();
                    if checked {
                        if !next.contains(opt_value) {
                            next.push(opt_value.clone());
                        }
                    } else {
                        next.retain(|v| v != opt_value);
                    }
                    p.push(PendingInteraction::DataUpdate {
                        path: path.clone(),
                        value: serde_json::json!(next),
                    });
                }
            }
        });
    } else {
        // Single selection — a native ComboBox dropdown.
        let selected_label = selected_values.first().and_then(|v| {
            options
                .iter()
                .find(|(_, val)| val == v)
                .map(|(lbl, _)| lbl.clone())
        });
        let mut picked: Option<String> = None;
        egui::ComboBox::from_id_salt(format!("{}_choice", ctx.component_id))
            .selected_text(selected_label.unwrap_or_default())
            .show_ui(ui, |ui| {
                for (opt_label, opt_value) in &options {
                    let is_sel = selected_values.first() == Some(opt_value);
                    if ui.selectable_label(is_sel, opt_label).clicked() {
                        picked = Some(opt_value.clone());
                    }
                }
            });
        if let Some(value) = picked
            && let Some(path) = &path
        {
            p.push(PendingInteraction::DataUpdate {
                path: path.clone(),
                value: serde_json::json!([value]),
            });
        }
    }
}

/// Tabs — a horizontal tab bar of clickable titles plus the active tab's child
/// panel. Unlike the other containers, Tabs does **not** use `child`/`children`;
/// it reads the `tabs` property (`Vec<{title, child}>`), where each `child` is a
/// component id.
///
/// The active index comes from the `activeTab` `DynamicNumber`. Clicking a tab
/// writes its index back to the `activeTab` binding (only when it is a
/// `Binding`; otherwise the bar renders + highlights the active tab and tracks
/// the selection locally via `walk.local_tabs` — the gallery samples don't bind
/// `activeTab`). Mirrors the Iced backend and the TUI reference.
pub(super) fn render_tabs(
    walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
) {
    let tabs = read_tabs(model);
    if tabs.is_empty() {
        return;
    }

    let active_dn = model.get_property::<DynamicNumber>("activeTab");
    // The write-back path, present only when activeTab is a data binding.
    let active_path: Option<String> = active_dn.as_ref().and_then(|dn| match dn {
        DynamicNumber::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    });

    // Resolve the active index: from the model when bound, else from the
    // locally-tracked selection.
    let active = match &active_dn {
        Some(dn) => ctx.data_context.resolve_dynamic_number(dn) as usize,
        None => walk.local_tabs.get(&ctx.component_id).copied().unwrap_or(0),
    }
    .min(tabs.len() - 1);

    // Tab bar — clickable titles; the active one is highlighted. A click writes
    // the index to the model when activeTab is bound, else tracks it locally.
    ui.horizontal(|ui| {
        for (i, (title, _child)) in tabs.iter().enumerate() {
            let is_active = i == active;
            let title_str = ctx.data_context.resolve_dynamic_string(title);
            if ui.selectable_label(is_active, &title_str).clicked() {
                match &active_path {
                    Some(path) => p.push(PendingInteraction::DataUpdate {
                        path: path.clone(),
                        value: serde_json::json!(i),
                    }),
                    None => p.push(PendingInteraction::TabActivate {
                        component_id: ctx.component_id.clone(),
                        index: i,
                    }),
                }
            }
        }
    });
    ui.separator();

    // Active tab's child panel.
    let active_child = tabs[active].1.clone();
    let child_base = ctx.data_context.base_path().to_string();
    render_child(walk, ui, eb, p, &active_child, &child_base);
}

/// Icon — maps an icon name to an emoji glyph, drawn in the embedded emoji
/// icon font (`a2ui-icons.ttf`, installed as a fallback by [`crate::EguiApp`]).
/// egui's default fonts have none of these glyphs, so every Icon relies on the
/// dedicated font; the mapping ([`map_icon_emoji`]) uses emoji codepoints (like
/// the Bevy backend), and unknown names fall back to `[xx]`.
pub(super) fn render_icon(ui: &mut Ui, ctx: &ComponentContext, model: &ComponentModel) {
    let name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    ui.label(
        egui::RichText::new(map_icon_emoji(&name))
            .size(18.0)
            .family(icon_family()),
    );
}

/// DateTimeInput — a native, editable ISO date/time field. egui ships no
/// calendar/clock widget, so the value is bound to a `text_edit_single_line`
/// (reusing the TextField chrome): the user types the ISO string and edits
/// write straight back to the data model — a genuinely interactive control
/// (not the read-only label this backend showed before). `enableDate` /
/// `enableTime` pick the placeholder hint shown to the left:
/// - both   → `YYYY-MM-DDTHH:MM:SS`
/// - date   → `YYYY-MM-DD`
/// - time   → `HH:MM:SS`
/// - neither → the raw ISO hint
///
/// The value is read from the `"value"` property (a `DynamicString`); when it
/// is a `Binding`, edits emit a [`PendingInteraction::DataUpdate`], mirroring
/// `render_text_field` (and the Iced backend). Mirrors the Iced/Bevy backends.
pub(super) fn render_date_time_input(
    _walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
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
    let enable_date: bool = model.get_property("enableDate").unwrap_or(true);
    let enable_time: bool = model.get_property("enableTime").unwrap_or(true);
    let hint = match (enable_date, enable_time) {
        (true, true) => "YYYY-MM-DDTHH:MM:SS",
        (true, false) => "YYYY-MM-DD",
        (false, true) => "HH:MM:SS",
        (false, false) => "ISO datetime",
    };

    if !label.is_empty() {
        ui.label(
            egui::RichText::new(format!("{label}: {hint}"))
                .weak()
                .small(),
        );
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

/// Image — renders a real decoded raster image (PNG / JPEG / …) from the cache
/// populated by [`crate::EguiApp::load_images`]. A resolved URL present in the
/// cache yields a `TextureHandle`; anything else (empty url, `data:` URL, decode
/// failure, not-yet-loaded) shows a labeled placeholder chip. The texture is
/// displayed at its natural size capped to a 480 px max width (preserving
/// aspect), matching the gallery-friendly sizing the other backends use.
pub(super) fn render_image(
    walk: &Walk<'_>,
    ui: &mut Ui,
    ctx: &ComponentContext,
    model: &ComponentModel,
) {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let description = model
        .get_property::<DynamicString>("description")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    if let Some(Some(handle)) = walk.image_cache.get(&url) {
        // Display at natural size capped to a 480 px width (aspect preserved).
        ui.add(egui::Image::from_texture(handle).max_width(480.0));
        return;
    }

    // Placeholder: empty / unsupported scheme / not-yet-loaded / failed decode.
    let label = if description.is_empty() {
        "image"
    } else {
        &description
    };
    ui.label(format!("🖼 image · {label}"));
}

/// Video / AudioPlayer — a labeled placeholder. egui (like the TUI/Iced/Slint/
/// Bevy backends) cannot play media; only the Dioxus WebView covers these.
pub(super) fn render_media_placeholder(
    ui: &mut Ui,
    kind: &str,
    ctx: &ComponentContext,
    model: &ComponentModel,
) {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let glyph = match kind {
        "Video" => "▷",
        "Audio" => "♪",
        _ => "◆",
    };
    ui.label(format!("{glyph} {kind} · {url}"));
}

// ===========================================================================
// Field helpers: tabs / choice / icon tables + resolvers
// ===========================================================================

/// One entry of a Tabs component's `tabs` property: a resolved title plus the
/// child component id to render when this tab is active. Built from raw
/// [`Value`]s (no `serde` derive — this crate only depends on `serde_json`):
/// `title` deserializes as the core `DynamicString`, `child` is a plain
/// component-id string. Mirrors the Iced backend.
fn read_tabs(model: &ComponentModel) -> Vec<(DynamicString, String)> {
    let Some(arr) = model.get_raw("tabs").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_tab_entry).collect()
}

/// Parse one `{title, child}` entry of the `tabs` array. `title` may be a
/// literal string or a data binding; `child` must be a component-id string.
/// Returns `None` (the entry is skipped) when either field is absent/malformed.
fn parse_tab_entry(v: &Value) -> Option<(DynamicString, String)> {
    let child = v.get("child")?.as_str()?.to_string();
    let title = serde_json::from_value::<DynamicString>(v.get("title")?.clone()).ok()?;
    Some((title, child))
}

/// An option entry in a ChoicePicker: the display label plus the value written
/// back when chosen. Built from raw [`Value`]s (no `serde` derive — see
/// [`read_tabs`]); `value` defaults to `""` (matching the TUI/Dioxus
/// `ChoiceOption`). Mirrors the Iced backend.
fn read_options(model: &ComponentModel) -> Vec<(String, String)> {
    let Some(arr) = model.get_raw("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_choice_option).collect()
}

/// Parse one `{label, value}` option of the `options` array. An entry missing a
/// label is skipped; a missing `value` defaults to an empty string.
fn parse_choice_option(v: &Value) -> Option<(String, String)> {
    let label = v.get("label")?.as_str()?.to_string();
    let value = v
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some((label, value))
}

/// Resolve a ChoicePicker's current selection as a `Vec<String>` from its
/// `value` `DynamicStringList` — accepting an array of strings or a single
/// string in the data model (mirroring the TUI/Iced backends).
fn resolve_choice_value(ctx: &ComponentContext, dsl: &DynamicStringList) -> Vec<String> {
    match dsl {
        DynamicStringList::Literal(v) => v.clone(),
        DynamicStringList::Binding(b) => match ctx.data_context.get(&b.path) {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        },
        DynamicStringList::Function(fc) => {
            match ctx
                .data_context
                .resolve_dynamic_value(&DynamicValue::Function(fc.clone()))
            {
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                Value::String(s) => vec![s],
                _ => Vec::new(),
            }
        }
    }
}

/// Map an A2UI icon name to an emoji glyph. Uses **emoji codepoints** so the
/// single embedded emoji font (`a2ui-icons.ttf`, installed as the `"Icons"`
/// family) covers every glyph — egui's default fonts have none of them. Covers
/// the same logical names the TUI / Iced backends do; unknown names fall back to
/// the first two characters in brackets. Mirrors the Bevy backend's
/// `map_icon_emoji`.
fn map_icon_emoji(name: &str) -> String {
    let glyph = match name {
        "mail" => "📧",
        "send" => "📤",
        "search" => "🔍",
        "settings" => "⚙",
        "star" => "⭐",
        "accountCircle" | "person" => "👤",
        "home" => "🏠",
        "heart" | "favorite" => "❤",
        "check" => "✅",
        "close" => "❌",
        "add" => "➕",
        "remove" => "➖",
        "edit" => "✏",
        "delete" => "🗑",
        "refresh" => "🔄",
        "arrowBack" => "⬅",
        "arrowForward" => "➡",
        "arrowUp" | "up" => "⬆",
        "arrowDown" | "down" => "⬇",
        "info" => "ℹ",
        "warning" => "⚠",
        "error" => "⛔",
        "success" => "✅",
        "calendarToday" => "📅",
        "locationOn" => "📍",
        "payment" => "💳",
        "phone" => "📞",
        "play" => "▶",
        "pause" => "⏸",
        "stop" => "⏹",
        "skipNext" | "next" => "⏭",
        "skipPrevious" | "previous" => "⏮",
        _ => return format!("[{}]", name.chars().take(2).collect::<String>()),
    };
    glyph.to_string()
}

/// The named `egui::FontFamily` the embedded emoji icon font is registered under
/// (see `EguiApp::install_icon_font`). Constructed via [`std::sync::Arc`] since
/// `FontFamily::Name` holds an `Arc<str>`.
pub(super) fn icon_family() -> egui::FontFamily {
    egui::FontFamily::Name(std::sync::Arc::from("Icons"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tab_entry_literal_title() {
        let v = serde_json::json!({ "title": "Overview", "child": "overview-col" });
        let (title, child) = parse_tab_entry(&v).expect("valid entry");
        assert_eq!(child, "overview-col");
        assert_eq!(title, DynamicString::Literal("Overview".to_string()));
    }

    #[test]
    fn parse_tab_entry_bound_title() {
        // A data-bound title deserializes into the Binding variant.
        let v = serde_json::json!({ "title": { "path": "/title" }, "child": "c1" });
        let (title, child) = parse_tab_entry(&v).expect("valid entry");
        assert_eq!(child, "c1");
        assert!(matches!(title, DynamicString::Binding(_)));
    }

    #[test]
    fn parse_tab_entry_missing_child_is_skipped() {
        // No `child` → the entry is skipped (returns None), not panicked.
        let v = serde_json::json!({ "title": "Overview" });
        assert!(parse_tab_entry(&v).is_none());
    }

    #[test]
    fn parse_choice_option_defaults_value_to_empty() {
        // `value` is optional in the spec — an option with only a label parses
        // with an empty value (matches the TUI/Dioxus `#[serde(default)]`).
        let v = serde_json::json!({ "label": "Code" });
        let (label, value) = parse_choice_option(&v).expect("valid option");
        assert_eq!(label, "Code");
        assert_eq!(value, "");
    }

    #[test]
    fn parse_choice_option_full() {
        let v = serde_json::json!({ "label": "Grand Ballroom", "value": "ballroom" });
        let (label, value) = parse_choice_option(&v).expect("valid option");
        assert_eq!(label, "Grand Ballroom");
        assert_eq!(value, "ballroom");
    }

    #[test]
    fn parse_choice_option_missing_label_is_skipped() {
        let v = serde_json::json!({ "value": "ballroom" });
        assert!(parse_choice_option(&v).is_none());
    }

    #[test]
    fn map_icon_emoji_known_name() {
        assert_eq!(map_icon_emoji("mail"), "📧");
        assert_eq!(map_icon_emoji("star"), "⭐");
        assert_eq!(map_icon_emoji("settings"), "⚙");
        // Aliases share a glyph.
        assert_eq!(map_icon_emoji("person"), map_icon_emoji("accountCircle"));
    }

    #[test]
    fn map_icon_emoji_unknown_falls_back_to_bracketed_prefix() {
        // Unknown names take the first two chars in brackets.
        assert_eq!(map_icon_emoji("XYZ"), "[XY]");
        assert_eq!(map_icon_emoji("k"), "[k]");
    }
}

/// Unknown / not-yet-implemented kind — show the kind name + recurse children.
pub(super) fn render_unknown(
    walk: &Walk<'_>,
    ui: &mut Ui,
    eb: &mut EditBuffers,
    p: &mut Vec<PendingInteraction>,
    ctx: &ComponentContext,
    model: &ComponentModel,
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
        Some(checks) => checks.iter().all(|rule| {
            ctx.data_context
                .resolve_dynamic_boolean_condition(&rule.condition)
        }),
        None => true,
    }
}
