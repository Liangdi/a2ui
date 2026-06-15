//! Recursive tree renderer — the Dioxus counterpart of the Iced
//! [`walker`](../../iced/src/walker.rs) + [`components`](../../iced/src/components.rs)
//! and the egui/ratatui `render_node` fns.
//!
//! Where the Iced backend builds an owned [`Element`] tree imperatively (an Elm
//! `view`) and the egui backend walks an immediate-mode `&mut Ui`, this module
//! is a single recursive Dioxus **component**, [`A2uiNode`]. Dioxus supports
//! recursive components natively (each call gets its own reactive scope — no
//! Slint-style bounded-depth codegen), so the whole A2UI tree is one component
//! that renders itself per node and re-enters itself for each child.
//!
//! The `MessageProcessor` (the framework-agnostic runtime that owns the
//! `id → ComponentModel` map) lives in a [`Signal`] at the gallery root and is
//! read here via [`use_context`]. Every `A2uiNode` subscribes to that signal by
//! reading it, so a write anywhere in the tree (a button activation, a field
//! edit) automatically re-renders the subscribers — **no Iced `Message` enum,
//! no egui `EditBuffers` bridge**. The signal *is* the interaction channel.
//!
//! Interactions split two ways:
//! - **Data writes** (TextField / Slider / CheckBox) write straight to the
//!   processor's data model via the shared signal — local, no indirection.
//! - **Button activation** runs the shared `dispatch_event` +
//!   `apply_event_result` core pipeline (plus the gallery's local Modal
//!   bookkeeping), which touches the `open_modals` set the root owns — so it is
//!   handed up through an `Rc<dyn Fn(String)>` context callback
//!   ([`crate::Gallery`]'s `handle_activate`).
//!
//! [`Element`]: dioxus::Element
//! [`Signal`]: dioxus::prelude::Signal
//! [`use_context`]: dioxus::prelude::use_context

use std::collections::HashMap;
use std::rc::Rc;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::protocol::common_types::{ChildList, DynamicBoolean, DynamicNumber, DynamicString};

use dioxus::prelude::*;

/// The merged function map shared with [`ComponentContext`], wrapped in `Rc` so
/// the non-`Clone` trait-object map can still be shared across components via
/// `use_context` (which requires `T: Clone`).
pub(crate) type Functions = HashMap<String, Box<dyn FunctionImplementation>>;

/// The activation callback handed up from a Button press to the gallery root.
/// Carries the activated component id; the root runs `dispatch_event` +
/// `apply_event_result` + the local Modal bookkeeping.
pub(crate) type OnActivate = Rc<dyn Fn(String)>;

/// Render one A2UI component node, recursively, into the Dioxus `Element` tree.
///
/// `id` is the component id in the surface's `id → ComponentModel` map;
/// `base_path` is the absolute JSON-Pointer prefix template children resolve
/// against (bindings are absolute per the A2UI convention). Both arrive as
/// component props so children are rendered as `<A2uiNode>` elements.
#[component]
pub fn A2uiNode(id: String, base_path: String) -> Element {
    let processor: Signal<MessageProcessor> = use_context();
    let functions: Rc<Functions> = use_context();
    let on_activate: OnActivate = use_context();
    let focused: Signal<Option<String>> = use_context();

    // Read scope: borrow the processor + its surface maps to read this node.
    // The signal read also subscribes us, so any write re-renders this node.
    let p = processor.read();
    let Some(surface) = p.model.surfaces().next() else {
        return rsx! { span { class: "unknown", "No surface loaded." } };
    };
    let components = surface.components.borrow();
    let data_model = surface.data_model.borrow();
    let Some(model) = components.get(&id) else {
        return rsx! { span { class: "unknown", "Component not found: {id}" } };
    };

    let focused_id = focused.read().as_deref().map(str::to_string);
    let ctx = ComponentContext::new(
        id.clone(),
        surface.id.clone(),
        &data_model,
        &components,
        functions.as_ref(),
        &base_path,
        focused_id,
    );

    // Dispatch to the matching render arm, mirroring the Iced/egui walkers.
    // Children re-enter via `<A2uiNode>` elements — those render in their own
    // scopes after this function returns (so the borrows above never overlap a
    // write), and a shared `RefCell::borrow` coexisting with another is fine.
    match model.component_type.as_str() {
        // Containers.
        "Column" | "List" => render_column(model, &ctx),
        "Row" => render_row(model, &ctx),
        "Card" => render_card(model, &ctx),
        "Tabs" => render_tabs(model, &ctx),
        "Modal" => render_modal(model, &ctx),

        // Content / leaf.
        "Text" => render_text(model, &ctx),
        "Divider" => render_divider(),
        "Icon" => render_icon(model, &ctx),
        "DateTimeInput" => render_date_time_input(model, &ctx),
        "Image" => render_media("Image", "▣", model, &ctx),
        "Video" => render_media("Video", "▷", model, &ctx),
        "AudioPlayer" => render_media("Audio", "♪", model, &ctx),

        // Interactive (native HTML controls).
        "Button" => render_button(model, &ctx, &on_activate),
        "TextField" => render_text_field(model, &ctx, processor),
        "CheckBox" => render_checkbox(model, &ctx, processor),
        "Slider" => render_slider(model, &ctx, processor),
        "ChoicePicker" => render_choice_picker(model, &ctx),

        _ => render_unknown(model, &ctx),
    }
}

// ===========================================================================
// Child planning — honor all three A2UI child shapes.
// ===========================================================================

/// Plan a node's children as `(child_id, child_base_path)` pairs, honoring all
/// three A2UI child shapes (`child`, static `children`, template `children`).
/// Mirrors `crates/iced/src/components.rs::build_child_plan`. Modal is handled
/// by its own renderer (trigger in-place; content as overlay), so its plan is
/// not used.
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

fn render_column(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let plan = build_child_plan(model, ctx);
    rsx! {
        div { class: "col",
            for (cid, base) in plan {
                A2uiNode { id: cid, base_path: base }
            }
        }
    }
}

fn render_row(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let plan = build_child_plan(model, ctx);
    rsx! {
        div { class: "row",
            for (cid, base) in plan {
                A2uiNode { id: cid, base_path: base }
            }
        }
    }
}

fn render_card(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let plan = build_child_plan(model, ctx);
    rsx! {
        div { class: "card col",
            for (cid, base) in plan {
                A2uiNode { id: cid, base_path: base }
            }
        }
    }
}

/// Modal — render its `trigger` child in-place. When open, the content floats
/// as a top-level overlay (built by [`crate::Gallery`]'s modal layer), so the
/// trigger keeps its place and focus.
fn render_modal(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    if let Some(trigger_id) = model.get_property::<String>("trigger") {
        rsx! { A2uiNode { id: trigger_id, base_path: ctx.data_context.base_path().to_string() } }
    } else {
        rsx! { span {} }
    }
}

fn render_tabs(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let active = model
        .get_property::<DynamicNumber>("activeTab")
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn))
        .unwrap_or(0.0) as usize;
    let plan = build_child_plan(model, ctx);
    if let Some((child_id, child_base)) = plan.into_iter().nth(active) {
        rsx! { A2uiNode { id: child_id, base_path: child_base } }
    } else {
        rsx! { span {} }
    }
}

// ===========================================================================
// Content / leaf
// ===========================================================================

fn render_text(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let content = model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let variant: Option<String> = model.get_property("variant");
    let class = match variant.as_deref() {
        Some("h1") => "text text--h1",
        Some("h2") => "text text--h2",
        Some("h3") => "text text--h3",
        _ => "text",
    };
    rsx! { div { class: "{class}", "{content}" } }
}

fn render_divider() -> Element {
    rsx! { hr {} }
}

fn render_icon(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    chip("◈", &format!("icon · {name}"))
}

fn render_date_time_input(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value = model
        .get_property::<DynamicString>("value")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    rsx! {
        div { class: "col",
            if !label.is_empty() {
                span { class: "muted", style: "font-size:12px", "{label}" }
            }
            div { class: "text", style: "font-size:13px", "{value}" }
        }
    }
}

/// Image / Video / AudioPlayer — a themed chip badge (real media in a later
/// pass; the WebView makes wiring real `<img>`/`<video>`/`<audio>` cheap, but
/// the samples don't ship media URLs, so P1 keeps the placeholder for parity
/// with the other backends).
fn render_media(kind: &str, glyph: &str, model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    chip(glyph, &format!("{kind} · {url}"))
}

// ===========================================================================
// Interactive (native HTML controls)
// ===========================================================================

/// Button — labeled press target. A press hands the component id up via the
/// `on_activate` context callback (the gallery root runs the shared
/// `dispatch_event` + `apply_event_result` pipeline + Modal bookkeeping).
fn render_button(model: &ComponentModel, ctx: &ComponentContext, on_activate: &OnActivate) -> Element {
    let label = resolve_child_text(ctx, model).unwrap_or_else(|| {
        model
            .accessibility()
            .and_then(|a| a.label)
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
            .unwrap_or_default()
    });
    let variant: Option<String> = model.get_property("variant");
    let disabled = !evaluate_checks(ctx, model);
    let class = match variant.as_deref() {
        Some("primary") => "btn btn--primary",
        Some("borderless") => "btn btn--borderless",
        _ => "btn",
    };
    // Capture an owned clone of the callback so the closure is `'static`.
    let cb = on_activate.clone();
    let id = ctx.component_id.clone();
    rsx! {
        button {
            class: "{class}",
            disabled,
            onclick: move |_| cb(id.clone()),
            "{label}"
        }
    }
}

/// TextField — controlled `<input>`, value resolved from the data model and
/// edits written straight back through the shared processor signal.
fn render_text_field(model: &ComponentModel, ctx: &ComponentContext, mut processor: Signal<MessageProcessor>) -> Element {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value_binding = model.get_property::<DynamicString>("value");
    let resolved = value_binding
        .as_ref()
        .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
        .unwrap_or_default();

    let path: Option<String> = match &value_binding {
        Some(DynamicString::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };

    rsx! {
        label { class: "field",
            if !label.is_empty() {
                span { class: "label", "{label}" }
            }
            input {
                value: "{resolved}",
                placeholder: "{label}",
                oninput: move |e| {
                    if let Some(path) = path.as_ref() {
                        let v = e.value();
                        let mut p = processor.write();
                        if let Some(surface) = p.model.surfaces_mut().next() {
                            surface.data_model.borrow_mut().set(path, serde_json::Value::String(v));
                        }
                    }
                },
            }
        }
    }
}

/// CheckBox — controlled checkbox; toggles write back through the signal.
fn render_checkbox(model: &ComponentModel, ctx: &ComponentContext, mut processor: Signal<MessageProcessor>) -> Element {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value_binding = model.get_property::<DynamicBoolean>("value");
    let resolved = value_binding
        .as_ref()
        .map(|db| ctx.data_context.resolve_dynamic_boolean(db))
        .unwrap_or(false);

    let path: Option<String> = match &value_binding {
        Some(DynamicBoolean::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };

    rsx! {
        label { class: "check",
            input {
                r#type: "checkbox",
                checked: "{resolved}",
                onchange: move |e| {
                    if let Some(path) = path.as_ref() {
                        let checked = e.checked();
                        let mut p = processor.write();
                        if let Some(surface) = p.model.surfaces_mut().next() {
                            surface.data_model.borrow_mut().set(path, serde_json::Value::Bool(checked));
                        }
                    }
                },
            }
            "{label}"
        }
    }
}

/// Slider — controlled range input; value changes write back through the signal.
fn render_slider(model: &ComponentModel, ctx: &ComponentContext, mut processor: Signal<MessageProcessor>) -> Element {
    let value_binding = model.get_property::<DynamicNumber>("value");
    let resolved = value_binding
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn))
        .unwrap_or(0.0) as f32;
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    let path_opt: Option<String> = match &value_binding {
        Some(DynamicNumber::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };

    rsx! {
        label { class: "field",
            if !label.is_empty() {
                span { class: "label", "{label}" }
            }
            input {
                class: "range",
                r#type: "range",
                min: "0",
                max: "100",
                value: "{resolved}",
                oninput: move |e| {
                    if let Some(path) = path_opt.as_ref() {
                        let v: f64 = e.value().parse().unwrap_or(0.0);
                        let mut p = processor.write();
                        if let Some(surface) = p.model.surfaces_mut().next() {
                            surface.data_model.borrow_mut().set(path, serde_json::json!(v));
                        }
                    }
                },
            }
        }
    }
}

/// ChoicePicker — a chip badge (a later pass wires a native `<select>`). Matches
/// the other backends' P1 scope.
fn render_choice_picker(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    chip("▾", &format!("select · {label}"))
}

/// Unknown / not-yet-implemented kind — show the kind name + recurse children.
fn render_unknown(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let kind = model.component_type.clone();
    let plan = build_child_plan(model, ctx);
    let header = chip("?", &format!("{kind} · unknown"))?;
    rsx! {
        div { class: "col",
            {header}
            for (cid, base) in plan {
                A2uiNode { id: cid, base_path: base }
            }
        }
    }
}

// ===========================================================================
// Field helpers
// ===========================================================================

/// A themed chip badge used for placeholder components (Icon / Image / Video /
/// AudioPlayer / ChoicePicker / unknown kinds) so they read as intentional
/// pills rather than bracket text.
fn chip(glyph: &str, label: &str) -> Element {
    rsx! {
        span { class: "chip",
            span { class: "chip__glyph", "{glyph}" }
            span { class: "chip__label", "{label}" }
        }
    }
}

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
