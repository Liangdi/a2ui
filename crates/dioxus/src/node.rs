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
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::protocol::common_types::{
    ChildList, DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList,
};

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
        "Tabs" => render_tabs(model, &ctx, processor),
        "Modal" => render_modal(model, &ctx),

        // Content / leaf.
        "Text" => render_text(model, &ctx),
        "Divider" => render_divider(),
        "Icon" => render_icon(model, &ctx),
        "DateTimeInput" => render_date_time_input(model, &ctx, processor),
        "Image" => render_image(model, &ctx),
        "Video" => render_video(model, &ctx),
        "AudioPlayer" => render_audio(model, &ctx),

        // Interactive (native HTML controls).
        "Button" => render_button(model, &ctx, &on_activate),
        "TextField" => render_text_field(model, &ctx, processor),
        "CheckBox" => render_checkbox(model, &ctx, processor),
        "Slider" => render_slider(model, &ctx, processor),
        "ChoicePicker" => render_choice_picker(model, &ctx, processor),

        _ => render_unknown(model, &ctx),
    }
}

/// A surface-id-aware variant of [`A2uiNode`].
///
/// [`A2uiNode`] assumes a *single* current surface (`processor.model
/// .surfaces().next()`) — correct for the gallery (one sample at a time) but
/// wrong for a chat, where each AI message is its own surface and many coexist.
/// This component takes an explicit `surface_id` and looks up *that* surface
/// (`processor.model.get_surface(&surface_id)`) instead of the first one, then
/// renders it with the exact same per-kind arms as `A2uiNode`. Children re-enter
/// via `<A2uiNodeInSurface>` (passing the same `surface_id` along) so the whole
/// subtree stays pinned to the right surface.
///
/// Used by the multi-surface `08_agent_chat` example; the gallery keeps using
/// [`A2uiNode`] unchanged.
#[component]
pub fn A2uiNodeInSurface(surface_id: String, id: String, base_path: String) -> Element {
    let processor: Signal<MessageProcessor> = use_context();
    let functions: Rc<Functions> = use_context();
    let on_activate: OnActivate = use_context();
    let focused: Signal<Option<String>> = use_context();

    // Read scope: borrow the processor + the named surface to read this node.
    // The signal read subscribes us, so any write re-renders this node.
    let p = processor.read();
    let Some(surface) = p.model.get_surface(&surface_id) else {
        return rsx! { span { class: "unknown", "missing surface {surface_id}" } };
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

    // Same dispatch as A2uiNode, but children re-enter as
    // `<A2uiNodeInSurface>` so the surface binding propagates down the tree.
    match model.component_type.as_str() {
        "Column" | "List" => render_column_in_surface(model, &ctx, &surface_id),
        "Row" => render_row_in_surface(model, &ctx, &surface_id),
        "Card" => render_card_in_surface(model, &ctx, &surface_id),
        "Tabs" => render_tabs_in_surface(model, &ctx, processor, &surface_id),
        "Modal" => render_modal_in_surface(model, &ctx, &surface_id),

        "Text" => render_text(model, &ctx),
        "Divider" => render_divider(),
        "Icon" => render_icon(model, &ctx),
        "DateTimeInput" => render_date_time_input(model, &ctx, processor),
        "Image" => render_image(model, &ctx),
        "Video" => render_video(model, &ctx),
        "AudioPlayer" => render_audio(model, &ctx),

        "Button" => render_button(model, &ctx, &on_activate),
        "TextField" => render_text_field(model, &ctx, processor),
        "CheckBox" => render_checkbox(model, &ctx, processor),
        "Slider" => render_slider(model, &ctx, processor),
        "ChoicePicker" => render_choice_picker(model, &ctx, processor),

        _ => render_unknown_in_surface(model, &ctx, &surface_id),
    }
}

// ===========================================================================
// Surface-pinned container arms — identical to the A2uiNode arms above, but
// children re-enter via `<A2uiNodeInSurface surface_id=..>` so a multi-surface
// host (e.g. the agent-chat example) keeps each subtree bound to its own
// surface. Kept inline (not genericized) to match the readability of the
// `A2uiNode` arms and avoid macro gymnastics.
// ===========================================================================

fn render_column_in_surface(
    model: &ComponentModel,
    ctx: &ComponentContext,
    surface_id: &str,
) -> Element {
    let plan = build_child_plan(model, ctx);
    let sid = surface_id.to_string();
    rsx! {
        div { class: "col",
            for (cid, base) in plan {
                A2uiNodeInSurface { surface_id: sid.clone(), id: cid, base_path: base }
            }
        }
    }
}

fn render_row_in_surface(
    model: &ComponentModel,
    ctx: &ComponentContext,
    surface_id: &str,
) -> Element {
    let plan = build_child_plan(model, ctx);
    let sid = surface_id.to_string();
    rsx! {
        div { class: "row",
            for (cid, base) in plan {
                A2uiNodeInSurface { surface_id: sid.clone(), id: cid, base_path: base }
            }
        }
    }
}

fn render_card_in_surface(
    model: &ComponentModel,
    ctx: &ComponentContext,
    surface_id: &str,
) -> Element {
    let plan = build_child_plan(model, ctx);
    let sid = surface_id.to_string();
    rsx! {
        div { class: "card col",
            for (cid, base) in plan {
                A2uiNodeInSurface { surface_id: sid.clone(), id: cid, base_path: base }
            }
        }
    }
}

/// Surface-pinned Modal — renders its `trigger` child in place (the content
/// floats as an overlay in single-surface hosts; the multi-surface chat does
/// not open modals, so an in-place trigger is enough).
fn render_modal_in_surface(
    model: &ComponentModel,
    ctx: &ComponentContext,
    surface_id: &str,
) -> Element {
    let sid = surface_id.to_string();
    if let Some(trigger_id) = model.get_property::<String>("trigger") {
        rsx! {
            A2uiNodeInSurface {
                surface_id: sid,
                id: trigger_id,
                base_path: ctx.data_context.base_path().to_string()
            }
        }
    } else {
        rsx! { span {} }
    }
}

/// Surface-pinned Tabs — same logic as `render_tabs`, but tab-bar writes target
/// the named surface and the active panel re-enters via `A2uiNodeInSurface`.
fn render_tabs_in_surface(
    model: &ComponentModel,
    ctx: &ComponentContext,
    mut processor: Signal<MessageProcessor>,
    surface_id: &str,
) -> Element {
    let tabs: Vec<TabEntry> = match model.get_property("tabs") {
        Some(t) => t,
        None => return rsx! { span {} },
    };
    if tabs.is_empty() {
        return rsx! { span {} };
    }

    let active = model
        .get_property::<DynamicNumber>("activeTab")
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn) as usize)
        .unwrap_or(0)
        .min(tabs.len() - 1);

    let active_path: Option<String> = model
        .get_property::<DynamicNumber>("activeTab")
        .as_ref()
        .and_then(|dn| match dn {
            DynamicNumber::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
            _ => None,
        });

    let sid = surface_id.to_string();
    // Pre-build owned per-tab items so the rsx `for` body stays a single
    // element and each onclick `move` closure owns its own `sid` clone (a
    // shared `String` would be moved out by the first closure). Mirrors the
    // pattern in `render_choice_picker`.
    let tab_items: Vec<(usize, String, &'static str, Option<String>, String)> = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let class = if i == active {
                "tab tab--active"
            } else {
                "tab"
            };
            (
                i,
                ctx.data_context.resolve_dynamic_string(&t.title),
                class,
                active_path.clone(),
                sid.clone(),
            )
        })
        .collect();

    let active_child = tabs[active].child.clone();
    let child_base = ctx.data_context.base_path().to_string();
    let panel_sid = sid.clone();

    rsx! {
        div { class: "tabs",
            div { class: "tabs__bar",
                for (idx, title, class, path_for_tab, tab_sid) in tab_items {
                    button {
                        class: "{class}",
                        onclick: move |_| {
                            if let Some(path) = path_for_tab.as_ref() {
                                let mut p = processor.write();
                                if let Some(surface) = p.model.get_surface_mut(&tab_sid) {
                                    surface.data_model.borrow_mut().set(path, serde_json::json!(idx));
                                }
                            }
                        },
                        "{title}"
                    }
                }
            }
            div { class: "tabs__panel",
                A2uiNodeInSurface { surface_id: panel_sid, id: active_child, base_path: child_base }
            }
        }
    }
}

/// Surface-pinned unknown-kind arm — recurses children via `A2uiNodeInSurface`.
fn render_unknown_in_surface(
    model: &ComponentModel,
    ctx: &ComponentContext,
    surface_id: &str,
) -> Element {
    let kind = model.component_type.clone();
    let plan = build_child_plan(model, ctx);
    let sid = surface_id.to_string();
    let header = chip("?", &format!("{kind} · unknown"))?;
    rsx! {
        div { class: "col",
            {header}
            for (cid, base) in plan {
                A2uiNodeInSurface { surface_id: sid.clone(), id: cid, base_path: base }
            }
        }
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

/// One entry of a Tabs component's `tabs` property: a resolved title plus the
/// child component id to show when this tab is active. Mirrors the TUI
/// reference (`crates/tui/src/components/tabs.rs::TabEntry`).
#[derive(Debug, Clone, serde::Deserialize)]
struct TabEntry {
    title: DynamicString,
    child: String,
}

/// Tabs — a horizontal tab bar plus a content panel. Unlike the other
/// containers, Tabs does **not** use `child`/`children`; it reads the `tabs`
/// property, a `Vec<{title, child}>`, where each `child` is a component id.
/// The active index comes from the `activeTab` `DynamicNumber`. Clicking a tab
/// writes its index back to the activeTab binding (only when it is a `Binding`;
/// otherwise the bar is read-only). Mirrors the TUI reference implementation.
fn render_tabs(
    model: &ComponentModel,
    ctx: &ComponentContext,
    mut processor: Signal<MessageProcessor>,
) -> Element {
    let tabs: Vec<TabEntry> = match model.get_property("tabs") {
        Some(t) => t,
        None => return rsx! { span {} },
    };
    if tabs.is_empty() {
        return rsx! { span {} };
    }

    // Resolve the active index, clamped to the last tab.
    let active = model
        .get_property::<DynamicNumber>("activeTab")
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn) as usize)
        .unwrap_or(0)
        .min(tabs.len() - 1);

    // The write-back path, only present when activeTab is a binding. When it
    // is absent, the tab bar still renders + highlights the active tab but
    // clicks do nothing (read-only), mirroring the TUI handle_event bail-out.
    let active_path: Option<String> = model
        .get_property::<DynamicNumber>("activeTab")
        .as_ref()
        .and_then(|dn| match dn {
            DynamicNumber::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
            _ => None,
        });

    // Pre-build owned per-tab items so the rsx `for` body stays a single
    // element (no leading `let`, no wrapping block) and each onclick closure
    // captures its own owned data. The class is pre-computed here so the loop
    // body is a plain `<button>`; `idx` is Copy (usize).
    let tab_items: Vec<(usize, String, &'static str, Option<String>)> = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let class = if i == active {
                "tab tab--active"
            } else {
                "tab"
            };
            (
                i,
                ctx.data_context.resolve_dynamic_string(&t.title),
                class,
                active_path.clone(),
            )
        })
        .collect();

    // Active tab's child (component id) + its base path.
    let active_child = tabs[active].child.clone();
    let child_base = ctx.data_context.base_path().to_string();

    rsx! {
        div { class: "tabs",
            div { class: "tabs__bar",
                for (idx, title, class, path_for_tab) in tab_items {
                    button {
                        class: "{class}",
                        onclick: move |_| {
                            if let Some(path) = path_for_tab.as_ref() {
                                let mut p = processor.write();
                                if let Some(surface) = p.model.surfaces_mut().next() {
                                    surface.data_model.borrow_mut().set(path, serde_json::json!(idx));
                                }
                            }
                        },
                        "{title}"
                    }
                }
            }
            div { class: "tabs__panel",
                A2uiNode { id: active_child, base_path: child_base }
            }
        }
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

/// Icon — maps an icon name to an emoji / unicode glyph (the WebView renders
/// emoji natively, so no icon font is needed). The mapping mirrors the TUI
/// backend's `map_icon` so every renderer agrees on the same symbol set;
/// unknown names fall back to the first two characters in brackets.
fn render_icon(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let glyph = map_icon(&name);
    rsx! { span { class: "icon", "{glyph}" } }
}

/// Map an A2UI icon name to an emoji / unicode glyph. Mirrors the TUI backend's
/// `map_icon` (`crates/tui/src/components/icon.rs`) for cross-backend parity.
fn map_icon(name: &str) -> String {
    let glyph = match name {
        "mail" => "✉",
        "send" => "➤",
        "search" => "🔍",
        "settings" => "⚙",
        "star" => "★",
        "accountCircle" => "👤",
        "home" => "🏠",
        "heart" => "♥",
        "check" => "✓",
        "close" => "✕",
        "add" => "+",
        "remove" => "−",
        "edit" => "✎",
        "delete" => "🗑",
        "refresh" => "⟳",
        "arrowBack" => "←",
        "arrowForward" => "→",
        "arrowUp" => "↑",
        "arrowDown" => "↓",
        "info" => "ℹ",
        "warning" => "⚠",
        "error" => "✗",
        "success" => "✔",
        _ => return format!("[{}]", name.chars().take(2).collect::<String>()),
    };
    glyph.to_string()
}

/// Adapt an ISO-ish data-model value into the substring the native HTML
/// `<input type=…>` expects for each kind. The data model stores full ISO
/// strings (e.g. `"2026-06-13T14:30:05"`); HTML inputs want trimmed forms:
/// - `datetime-local` → `"YYYY-MM-DDTHH:MM"` (no seconds)
/// - `date`           → `"YYYY-MM-DD"`
/// - `time`           → `"HH:MM"`
/// - `text` (degraded)→ the raw string unchanged
///
/// Bounds-checked throughout so a short/garbage value never panics.
fn adapt_html_value(val: &str, kind: &str) -> String {
    let len = val.len();
    match kind {
        "datetime-local" => val.get(..16.min(len)).unwrap_or(val).to_string(),
        "date" => val.get(..10.min(len)).unwrap_or(val).to_string(),
        "time" => {
            // After the 'T' separator take 5 chars ("HH:MM"); if there's no
            // 'T', treat the whole string as a time.
            match val.find('T') {
                Some(idx) => {
                    let after = &val[idx + 1..];
                    after.get(..5.min(after.len())).unwrap_or(after).to_string()
                }
                None => val.get(..5.min(len)).unwrap_or(val).to_string(),
            }
        }
        _ => val.to_string(),
    }
}

/// DateTimeInput — a native HTML date/time picker driven by `enableDate` /
/// `enableTime`:
/// - both   → `<input type="datetime-local">`
/// - date   → `<input type="date">`
/// - time   → `<input type="time">`
/// - neither → `<input type="text">` (degraded, mirrors the TUI fallback).
///
/// The value is read from the `"value"` property (a `DynamicString`). When it
/// is a `Binding`, edits write straight back through the shared processor
/// signal — same pattern as `render_text_field`. A non-binding value is shown
/// read-only.
fn render_date_time_input(
    model: &ComponentModel,
    ctx: &ComponentContext,
    mut processor: Signal<MessageProcessor>,
) -> Element {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    let value_binding = model.get_property::<DynamicString>("value");
    let resolved = value_binding
        .as_ref()
        .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
        .unwrap_or_default();

    let enable_date = model.get_property::<bool>("enableDate").unwrap_or(true);
    let enable_time = model.get_property::<bool>("enableTime").unwrap_or(true);

    let kind: &str = match (enable_date, enable_time) {
        (true, true) => "datetime-local",
        (true, false) => "date",
        (false, true) => "time",
        (false, false) => "text",
    };
    let html_value = adapt_html_value(&resolved, kind);

    // A value is only writable when it is a Binding; resolve its absolute path.
    let path: Option<String> = match &value_binding {
        Some(DynamicString::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };
    let readonly = path.is_none();

    rsx! {
        label { class: "field",
            if !label.is_empty() {
                span { class: "label", "{label}" }
            }
            input {
                r#type: "{kind}",
                value: "{html_value}",
                placeholder: "{label}",
                readonly,
                onchange: move |e| {
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

/// Video — a native `<video>` element. The WebView plays it natively with full
/// transport controls (play / pause / seek / volume / fullscreen) — something
/// the terminal backend can never do. An empty `url` falls back to the
/// placeholder chip; an optional `posterUrl` sets the poster frame.
fn render_video(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let description = model
        .get_property::<DynamicString>("description")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let poster = model
        .get_property::<DynamicString>("posterUrl")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    if url.is_empty() {
        let label = if description.is_empty() {
            "video"
        } else {
            &description
        };
        return chip("▷", &format!("video · {label}"));
    }

    rsx! {
        figure { class: "video",
            video {
                class: "video__el",
                src: "{url}",
                controls: true,
                poster: "{poster}",
            }
            if !description.is_empty() {
                figcaption { class: "video__cap", "{description}" }
            }
        }
    }
}

/// AudioPlayer — a native `<audio controls>` element. The WebView plays it
/// natively with full transport controls — unlike the terminal backend, which
/// needs the `audio` feature + `rodio` + ALSA. An empty `url` falls back to
/// the placeholder chip.
fn render_audio(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let description = model
        .get_property::<DynamicString>("description")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    if url.is_empty() {
        let label = if description.is_empty() {
            "audio"
        } else {
            &description
        };
        return chip("♪", &format!("audio · {label}"));
    }

    rsx! {
        div { class: "audio",
            if !description.is_empty() {
                span { class: "audio__label", "{description}" }
            }
            audio {
                class: "audio__el",
                src: "{url}",
                controls: true,
            }
        }
    }
}

/// Image — native `<img>`. The WebView supports `file://`, `http(s)`, and
/// `data:` URLs natively, so unlike the other backends the Dioxus gallery shows
/// the real picture. An empty `url` falls back to the placeholder chip.
fn render_image(model: &ComponentModel, ctx: &ComponentContext) -> Element {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let description = model
        .get_property::<DynamicString>("description")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let fit: Option<String> = model.get_property("fit");

    // Empty URL → keep the placeholder chip for parity with the other backends.
    if url.is_empty() {
        let label = if description.is_empty() {
            "image"
        } else {
            &description
        };
        return chip("🖼", &format!("image · {label}"));
    }

    // Map the A2UI `fit` hint onto CSS `object-fit`; default to contain.
    let object_fit = match fit.as_deref() {
        Some("cover") | Some("fill") | Some("none") | Some("scale-down") => fit.as_deref().unwrap(),
        _ => "contain",
    };

    rsx! {
        figure { class: "image",
            img {
                class: "image__img",
                src: "{url}",
                alt: "{description}",
                style: "object-fit: {object_fit};",
            }
            if !description.is_empty() {
                figcaption { class: "image__cap", "{description}" }
            }
        }
    }
}

// ===========================================================================
// Interactive (native HTML controls)
// ===========================================================================

/// Button — labeled press target. A press hands the component id up via the
/// `on_activate` context callback (the gallery root runs the shared
/// `dispatch_event` + `apply_event_result` pipeline + Modal bookkeeping).
fn render_button(
    model: &ComponentModel,
    ctx: &ComponentContext,
    on_activate: &OnActivate,
) -> Element {
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
fn render_text_field(
    model: &ComponentModel,
    ctx: &ComponentContext,
    mut processor: Signal<MessageProcessor>,
) -> Element {
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
fn render_checkbox(
    model: &ComponentModel,
    ctx: &ComponentContext,
    mut processor: Signal<MessageProcessor>,
) -> Element {
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
fn render_slider(
    model: &ComponentModel,
    ctx: &ComponentContext,
    mut processor: Signal<MessageProcessor>,
) -> Element {
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

/// An option entry in a ChoicePicker (matches the TUI/egui backends'
/// `ChoiceOption`).
#[derive(Debug, Clone, serde::Deserialize)]
struct ChoiceOption {
    label: String,
    #[serde(default)]
    value: String,
}

/// ChoicePicker — a list of selectable options.
///
/// - Single selection (`variant == "mutuallyExclusive"` or default) renders a
///   native `<select>`; the chosen option writes back as `json!([value])` (an
///   array, matching the TUI backend's `EventResult`).
/// - Multiple selection (`variant == "multipleSelection"`) renders a set of
///   `<input type="checkbox">` rows; toggling adds/removes the value in the
///   array written back.
///
/// Only a `Binding` `value` is writable; a `Literal`/`Function` value degrades
/// to a read-only control (disabled), matching how the TUI `handle_event`
/// bails on non-binding values.
fn render_choice_picker(
    model: &ComponentModel,
    ctx: &ComponentContext,
    mut processor: Signal<MessageProcessor>,
) -> Element {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    let options: Vec<ChoiceOption> = model.get_property("options").unwrap_or_default();

    // Resolve the current selection as Vec<String>, mirroring the TUI backend.
    let value_binding = model.get_property::<DynamicStringList>("value");
    let selected_values: Vec<String> = match &value_binding {
        Some(DynamicStringList::Literal(v)) => v.clone(),
        Some(DynamicStringList::Binding(b)) => match ctx.data_context.get(&b.path) {
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            Some(serde_json::Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        },
        Some(DynamicStringList::Function(_)) | None => Vec::new(),
    };

    // Only a Binding is writable. Literal/Function/None -> read-only.
    let path: Option<String> = match &value_binding {
        Some(DynamicStringList::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };
    let readonly = path.is_none();
    let is_multiple = model
        .get_property::<String>("variant")
        .as_deref()
        .map(|v| v == "multipleSelection")
        .unwrap_or(false);

    if is_multiple {
        // Pre-build owned rows — each carries its own `path` clone so every
        // onchange `move` closure owns its copy (a shared Option<String> would
        // be moved out by the first closure). rsx `for` body stays a single
        // element this way too.
        let rows: Vec<(String, String, bool, Option<String>)> = options
            .iter()
            .map(|o| {
                (
                    o.label.clone(),
                    o.value.clone(),
                    selected_values.contains(&o.value),
                    path.clone(),
                )
            })
            .collect();
        rsx! {
            div { class: "field",
                if !label.is_empty() {
                    span { class: "label", "{label}" }
                }
                div { class: "choice",
                    for (lbl, val, checked, row_path) in rows {
                        label { class: "check",
                            input {
                                r#type: "checkbox",
                                checked: "{checked}",
                                disabled: readonly,
                                onchange: move |e| {
                                    let Some(path) = row_path.as_ref() else { return; };
                                    let this_val = val.clone();
                                    let now_checked = e.checked();
                                    let mut p = processor.write();
                                    if let Some(surface) = p.model.surfaces_mut().next() {
                                        let mut dm = surface.data_model.borrow_mut();
                                        let current: Vec<String> = match dm.get(path) {
                                            Some(serde_json::Value::Array(arr)) => arr
                                                .iter()
                                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                                .collect(),
                                            Some(serde_json::Value::String(s)) => vec![s.clone()],
                                            _ => Vec::new(),
                                        };
                                        let next: Vec<String> = if now_checked {
                                            if !current.contains(&this_val) {
                                                let mut c = current;
                                                c.push(this_val);
                                                c
                                            } else {
                                                current
                                            }
                                        } else {
                                            current.into_iter().filter(|v| v != &this_val).collect()
                                        };
                                        dm.set(path, serde_json::json!(next));
                                    }
                                },
                            }
                            "{lbl}"
                        }
                    }
                }
            }
        }
    } else {
        // Single selection: a native `<select>`, controlled via its `value`
        // attribute (the browser matches it to the child `<option value>`), so
        // no per-option `selected` boolean attribute is needed.
        let current = selected_values.first().cloned().unwrap_or_default();
        let opts: Vec<(String, String)> = options
            .iter()
            .map(|o| (o.label.clone(), o.value.clone()))
            .collect();
        let path_for = path;
        rsx! {
            label { class: "field",
                if !label.is_empty() {
                    span { class: "label", "{label}" }
                }
                select {
                    class: "choice__select",
                    value: "{current}",
                    disabled: readonly,
                    onchange: move |e| {
                        if let Some(path) = path_for.as_ref() {
                            let v = e.value();
                            let mut p = processor.write();
                            if let Some(surface) = p.model.surfaces_mut().next() {
                                surface.data_model.borrow_mut().set(path, serde_json::json!([v]));
                            }
                        }
                    },
                    for (lbl, val) in opts {
                        option { value: "{val}", "{lbl}" }
                    }
                }
            }
        }
    }
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
        Some(checks) => checks.iter().all(|rule| {
            ctx.data_context
                .resolve_dynamic_boolean_condition(&rule.condition)
        }),
        None => true,
    }
}

// ===========================================================================
// Tests — the pure helpers (icon mapping, date-string adaptation, and the
// `tabs`/`options` struct deserialization) are independent of the WebView, so
// they can be exercised without launching a window.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_icon_known_names_match_tui() {
        assert_eq!(map_icon("mail"), "✉");
        assert_eq!(map_icon("settings"), "⚙");
        assert_eq!(map_icon("home"), "🏠");
        assert_eq!(map_icon("search"), "🔍");
        assert_eq!(map_icon("success"), "✔");
    }

    #[test]
    fn map_icon_fallback_is_first_two_chars() {
        assert_eq!(map_icon("xyz"), "[xy]");
        assert_eq!(map_icon("a"), "[a]");
        assert_eq!(map_icon(""), "[]");
    }

    #[test]
    fn adapt_html_value_datetime_local_strips_seconds() {
        assert_eq!(
            adapt_html_value("2026-06-13T14:30:05", "datetime-local"),
            "2026-06-13T14:30"
        );
    }

    #[test]
    fn adapt_html_value_date_is_first_ten() {
        assert_eq!(
            adapt_html_value("2026-06-13T14:30:05", "date"),
            "2026-06-13"
        );
    }

    #[test]
    fn adapt_html_value_time_strips_after_t() {
        assert_eq!(adapt_html_value("2026-06-13T14:30:05", "time"), "14:30");
        // No 'T' separator: treat the whole string as a time.
        assert_eq!(adapt_html_value("09:15:00", "time"), "09:15");
    }

    #[test]
    fn adapt_html_value_short_input_never_panics() {
        assert_eq!(adapt_html_value("2026", "date"), "2026");
        assert_eq!(adapt_html_value("", "datetime-local"), "");
        assert_eq!(adapt_html_value("x", "time"), "x");
        // The `text` (degraded) kind passes the value through unchanged.
        assert_eq!(adapt_html_value("anything", "text"), "anything");
    }

    #[test]
    fn choice_option_deserializes_without_value() {
        // `value` has #[serde(default)] — an option with only a label must parse.
        let json = serde_json::json!({ "label": "Code" });
        let opt: ChoiceOption = serde_json::from_value(json).unwrap();
        assert_eq!(opt.label, "Code");
        assert_eq!(opt.value, "");
    }

    #[test]
    fn tab_entry_deserializes_child_id() {
        // Mirrors the spec `tabs_checks.json` valid case.
        let json = serde_json::json!({ "title": "Tab 1", "child": "txt1" });
        let entry: TabEntry = serde_json::from_value(json).unwrap();
        assert_eq!(entry.child, "txt1");
    }
}
