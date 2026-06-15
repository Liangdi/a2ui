//! Per-component-kind Iced render functions.
//!
//! Each `render_*` fn reads the pieces a component needs from the A2UI models
//! and returns an [`Element`] tree. Interactive widgets attach a
//! [`Message`](crate::Message) (via `.on_press` / `.on_input` / …) that
//! [`IcedApp::update`](crate::IcedApp) applies back to the runtime after the
//! view returns. Container fns re-enter [`crate::walker::render_node`] for
//! their children, mirroring the egui/ratatui `render_node` recursion.
//!
//! ## Lifetime note
//!
//! Every Iced widget **owns** its content — `text(String)`,
//! `button(text(String))`, and crucially `text_input(placeholder, value)`
//! (the `&str`s are copied into owned `String`/`Value` storage; only the
//! `on_*` closures borrow, and those capture owned `Message` values). So the
//! returned `Element<'a, Message>` borrows nothing from the inputs; `'a` is
//! effectively unconstrained. This is why no egui-style `EditBuffers` state
//! bridge is needed: we resolve dynamic values to owned `String`s/f64s/bools
//! and hand them to stateless widgets, with write-back flowing through
//! `Message`s instead of `&mut` buffers.

use std::collections::HashMap;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{ChildList, DynamicBoolean, DynamicNumber, DynamicString};

use iced::widget::{Column, Row};
use iced::widget::{button, checkbox, container, rule, slider, text, text_input};
use iced::{Color, Element, Fill};

use crate::message::Message;
use crate::walker::render_node;

/// Shared read-only context threaded through every render function. This is
/// the Iced counterpart of the egui `Walk` struct (minus `open_modals`, which
/// the walker doesn't need — the Modal overlay is built separately in
/// [`crate::IcedApp::view`]).
pub(super) struct Walk<'a> {
    pub surface_id: &'a str,
    pub data_model: &'a DataModel,
    pub components: &'a SurfaceComponentsModel,
    pub functions: &'a HashMap<String, Box<dyn FunctionImplementation>>,
    pub focused_id: Option<&'a str>,
}

/// Re-enter the walker for one child, returning its element.
fn render_child<'a>(
    walk: &Walk<'_>,
    child_id: &str,
    base_path: &str,
) -> Element<'a, Message> {
    render_node(
        child_id,
        walk.surface_id,
        base_path,
        walk.data_model,
        walk.components,
        walk.functions,
        walk.focused_id,
    )
}

/// Plan a node's children as `(child_id, child_base_path)` pairs, honoring all
/// three A2UI child shapes (`child`, static `children`, template `children`).
///
/// Mirrors `crates/egui/src/components.rs::build_child_plan` and the Slint
/// `live_tree::build_child_plan`. Modal is handled by its own renderer (trigger
/// in-place; content as overlay), so it is excluded.
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

/// Build the child elements of a container node as a `Vec<Element>`.
fn build_children<'a>(walk: &Walk<'_>, model: &ComponentModel, ctx: &ComponentContext) -> Vec<Element<'a, Message>> {
    build_child_plan(model, ctx)
        .into_iter()
        .map(|(cid, base)| render_child(walk, &cid, &base))
        .collect()
}

// ===========================================================================
// Containers
// ===========================================================================

/// Column / List — vertical stack of children.
pub(super) fn render_column<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let children = build_children(walk, model, ctx);
    Column::with_children(children)
        .spacing(8.0)
        .width(Fill)
        .into()
}

/// Row — horizontal stack of children.
pub(super) fn render_row<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let children = build_children(walk, model, ctx);
    Row::with_children(children)
        .spacing(8.0)
        .into()
}

/// Card — bordered panel wrapping its children.
pub(super) fn render_card<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let children = build_children(walk, model, ctx);
    let inner = Column::with_children(children).spacing(6.0);
    container(inner)
        .padding(10.0)
        .width(Fill)
        .style(container::bordered_box)
        .into()
}

/// Modal — render its `trigger` child in-place. When open, the content floats
/// as a top-level overlay (built by [`crate::IcedApp::view`] via a `Stack`
/// after the main tree), so the trigger keeps its place and focus.
pub(super) fn render_modal<'a>(
    walk: &Walk<'_>, _ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    if let Some(trigger_id) = model.get_property::<String>("trigger") {
        render_child(walk, &trigger_id, "")
    } else {
        text("").into()
    }
}

/// Tabs — render the active child panel (P2 adds the tab bar).
pub(super) fn render_tabs<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let active = model
        .get_property::<DynamicNumber>("activeTab")
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn))
        .unwrap_or(0.0) as usize;
    let plan = build_child_plan(model, ctx);
    if let Some((child_id, child_base)) = plan.get(active) {
        render_child(walk, child_id, child_base)
    } else {
        text("").into()
    }
}

// ===========================================================================
// Content / leaf
// ===========================================================================

/// Text — styled label; `variant` h1/h2/h3 select heading sizes.
pub(super) fn render_text<'a>(ctx: &ComponentContext, model: &ComponentModel) -> Element<'a, Message> {
    let content = model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let variant: Option<String> = model.get_property("variant");
    let mut t = text(content);
    match variant.as_deref() {
        Some("h1") => t = t.size(28.0),
        Some("h2") => t = t.size(22.0),
        Some("h3") => t = t.size(18.0),
        _ => {}
    }
    t.into()
}

/// Divider — thin horizontal rule.
pub(super) fn render_divider<'a>() -> Element<'a, Message> {
    rule::horizontal(1.0).into()
}

/// Icon — labeled box (no icon font); placeholder.
pub(super) fn render_icon<'a>(ctx: &ComponentContext, model: &ComponentModel) -> Element<'a, Message> {
    let name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    text(format!("[icon: {name}]")).into()
}

/// DateTimeInput — bordered field showing label + value (P2 full impl).
pub(super) fn render_date_time_input<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value = model
        .get_property::<DynamicString>("value")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    text(format!("{label}: {value}")).into()
}

/// Image / Video / AudioPlayer — labeled placeholder (real texture/audio in P3).
pub(super) fn render_media_placeholder<'a>(
    kind: &str, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    text(format!("[{kind}: {url}]")).into()
}

// ===========================================================================
// Interactive (native Iced widgets)
// ===========================================================================

/// Button — labeled press target. A press dispatches `Enter` to its component
/// via the core pipeline (reuses [`crate::dispatch_event`] +
/// [`crate::apply_event_result`] in `update`), like the egui/Slint hosts'
/// `handle_activate`. The label is the Button's single `child` (a Text).
pub(super) fn render_button<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let label = resolve_child_text(ctx, model).unwrap_or_else(|| {
        model
            .accessibility()
            .and_then(|a| a.label)
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
            .unwrap_or_default()
    });
    let variant: Option<String> = model.get_property("variant");
    let checks_pass = evaluate_checks(ctx, model);

    let btn = button(text(label));
    let btn = match variant.as_deref() {
        Some("primary") => btn.style(button::primary),
        Some("borderless") => btn.style(button::text),
        _ => btn,
    };
    // Disable the press target when any `checks` rule fails. A non-pressable
    // button still renders its label (iced handles the disabled appearance).
    let activate = if checks_pass {
        Some(Message::ButtonActivate {
            component_id: ctx.component_id.clone(),
        })
    } else {
        None
    };
    btn.on_press_maybe(activate).into()
}

/// TextField — Iced native single-line edit, bridged to the data model. The
/// value is resolved from the model each frame (owned, copied into the widget);
/// edits emit a [`Message::DataUpdate`] carrying the absolute binding path.
pub(super) fn render_text_field<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value_binding = model.get_property::<DynamicString>("value");
    let resolved = value_binding
        .as_ref()
        .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
        .unwrap_or_default();

    let on_change = match &value_binding {
        Some(DynamicString::Binding(b)) => {
            let path = ctx.data_context.resolve_pointer(&b.path);
            Some(move |s: String| Message::DataUpdate {
                path: path.clone(),
                value: serde_json::Value::String(s),
            })
        }
        _ => None,
    };

    let mut col = Column::new().spacing(2.0);
    if !label.is_empty() {
        col = col.push(
            text(label.clone())
                .size(12.0)
                .color(Color::from_rgb(0.45, 0.45, 0.45)),
        );
    }
    col = col.push(text_input(&label, &resolved).on_input_maybe(on_change));
    col.into()
}

/// CheckBox — Iced native checkbox; toggles write back to the data model.
pub(super) fn render_checkbox<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value_binding = model.get_property::<DynamicBoolean>("value");
    let resolved = value_binding
        .as_ref()
        .map(|db| ctx.data_context.resolve_dynamic_boolean(db))
        .unwrap_or(false);

    let on_toggle = match &value_binding {
        Some(DynamicBoolean::Binding(b)) => {
            let path = ctx.data_context.resolve_pointer(&b.path);
            Some(move |checked: bool| Message::DataUpdate {
                path: path.clone(),
                value: serde_json::Value::Bool(checked),
            })
        }
        _ => None,
    };

    checkbox(resolved)
        .label(label)
        .on_toggle_maybe(on_toggle)
        .into()
}

/// Slider — Iced native slider; value changes write back to the data model.
pub(super) fn render_slider<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let value_binding = model.get_property::<DynamicNumber>("value");
    let resolved = value_binding
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn))
        .unwrap_or(0.0) as f32;
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    // `slider` requires an `on_change` even when the value isn't bound to the
    // model; capture an `Option<path>` and emit an empty-path `DataUpdate`
    // (which `update` ignores) when unbound.
    let path_opt: Option<String> = match &value_binding {
        Some(DynamicNumber::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };
    let track = slider(0.0..=100.0, resolved, move |v| Message::DataUpdate {
        path: path_opt.clone().unwrap_or_default(),
        value: serde_json::json!(v as f64),
    });

    let mut col = Column::new().spacing(2.0);
    if !label.is_empty() {
        col = col.push(text(label).size(12.0).color(Color::from_rgb(0.45, 0.45, 0.45)));
    }
    col = col.push(track);
    col.into()
}

/// ChoicePicker — placeholder label (P2 wires a native `pick_list`). Matches
/// the egui backend's P1 scope.
pub(super) fn render_choice_picker<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    text(format!("[ChoicePicker: {label}]")).into()
}

/// Unknown / not-yet-implemented kind — show the kind name + recurse children.
pub(super) fn render_unknown<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let header = text(format!("[{}]", model.component_type));
    let mut col = Column::new().spacing(4.0).push(header);
    for child in build_children(walk, model, ctx) {
        col = col.push(child);
    }
    col.into()
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
