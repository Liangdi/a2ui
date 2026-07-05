//! Per-component-kind GPUI render functions.
//!
//! Each `render_*` fn reads the pieces a component needs from the A2UI models
//! and returns an [`AnyElement`] tree. Stateless interactive widgets (Button,
//! Checkbox, TabBar, list rows) attach `cx.listener` closures that fire *after*
//! the render borrow is released and mutate the runtime directly — no egui-style
//! collect-then-apply buffer. Stateful widgets (TextField / Slider /
//! ChoicePicker) own a persistent `Entity<…State>` that GPUI keeps alive across
//! frames; these are lazily created on first render into the `RefCell` caches on
//! [`Walk`] (keyed by component id), seeded from the data model, and subscribed
//! once for write-back. Container fns re-enter [`crate::walker::render_node`]
//! for their children, mirroring the iced/egui/ratatui `render_node` recursion.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{
    ChildList, DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList, DynamicValue,
};
use serde_json::Value;

use gpui::prelude::*;
use gpui::{AnyElement, ClickEvent, Context, Entity, Image, ObjectFit, SharedString, Subscription, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::divider::Divider;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{Disableable as _, IndexPath, h_flex, v_flex};

use crate::app::GpuiApp;
use crate::images;
use crate::theme;
use crate::walker::render_node;

/// Shared read-only + `RefCell`-mutable context threaded through every render
/// function. The read-only model refs (`data_model`, `components`, `functions`,
/// `focused_id`, `image_cache`, `local_tabs`) are what [`ComponentContext`]
/// borrows; the `RefCell` caches ([`text_states`] / [`slider_states`] /
/// [`select_states`] / [`subscriptions`] / [`image_pending`]) give the
/// otherwise-immutable `&Walk` the interior mutability it needs to lazily
/// create state entities during the recursive walk.
///
/// [`text_states`]: Walk::text_states
/// [`slider_states`]: Walk::slider_states
/// [`select_states`]: Walk::select_states
/// [`subscriptions`]: Walk::subscriptions
/// [`image_pending`]: Walk::image_pending
pub(super) struct Walk<'a> {
    pub surface_id: &'a str,
    pub data_model: &'a DataModel,
    pub components: &'a SurfaceComponentsModel,
    pub functions: &'a HashMap<String, Box<dyn FunctionImplementation>>,
    pub focused_id: Option<&'a str>,
    /// Remote-image cache: a resolved URL → its decoded gpui [`Image`] once the
    /// background fetch completes (`None` = attempted but failed, so it isn't
    /// refetched). Local-file images go through the same resolve→decode path
    /// (uniform handling); only `http(s)` / `data:` / `file://` URLs enter here.
    pub image_cache: &'a HashMap<String, Option<Arc<Image>>>,
    /// URLs a background fetch is already in flight for, so a not-yet-decoded
    /// image isn't refetched every frame.
    pub image_pending: &'a RefCell<HashSet<String>>,
    /// Locally-tracked active tab index for Tabs components whose `activeTab`
    /// is **not** a data binding (the gallery samples fall here). Keyed by
    /// component id. Bound Tabs write to the model instead.
    pub local_tabs: &'a HashMap<String, usize>,
    /// Per-component-id `InputState` for TextField / DateTimeInput, lazily
    /// created on first render.
    pub text_states: &'a RefCell<HashMap<String, Entity<InputState>>>,
    /// Per-component-id `SliderState` for Slider, lazily created on first render.
    pub slider_states: &'a RefCell<HashMap<String, Entity<SliderState>>>,
    /// Per-component-id `SelectState` for single-select ChoicePicker, lazily
    /// created on first render.
    pub select_states: &'a RefCell<HashMap<String, Entity<SelectState<SearchableVec<String>>>>>,
    /// Keeps every lazy-create subscription alive for the app's lifetime
    /// (dropping a [`Subscription`] unsubscribes it).
    pub subscriptions: &'a RefCell<Vec<Subscription>>,
}

/// Re-enter the walker for one child at `base_path`, returning its element.
fn render_child(
    walk: &Walk,
    child_id: &str,
    base_path: &str,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    render_node(child_id, base_path, walk, window, cx)
}

/// Plan a node's children as `(child_id, child_base_path)` pairs, honoring all
/// three A2UI child shapes (`child`, static `children`, template `children`).
///
/// Mirrors `crates/iced/src/components.rs::build_child_plan` and the egui
/// `build_child_plan`. Modal is handled by its own renderer (trigger in-place;
/// content as overlay), so it is excluded.
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
            if let Some(Value::Array(arr)) = ctx.data_context.get(&path) {
                for i in 0..arr.len() {
                    plan.push((component_id.clone(), format!("{path}/{i}")));
                }
            }
        }
        None => {}
    }
    plan
}

/// Build the child elements of a container node as a `Vec<AnyElement>`. A plain
/// `for` loop (not `.map().collect()`) so the `&mut Window` / `&mut Context`
/// re-borrow cleanly across iterations without closing over them.
fn build_children(
    walk: &Walk,
    model: &ComponentModel,
    ctx: &ComponentContext,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> Vec<AnyElement> {
    let plan = build_child_plan(model, ctx);
    let mut out = Vec::with_capacity(plan.len());
    for (cid, base) in plan {
        out.push(render_child(walk, &cid, &base, window, cx));
    }
    out
}

// ===========================================================================
// Containers
// ===========================================================================

/// Column / List — vertical stack of children.
pub(super) fn render_column(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    v_flex().gap_2().w_full().children(build_children(walk, model, ctx, window, cx)).into_any_element()
}

/// Row — horizontal stack of children.
pub(super) fn render_row(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    h_flex().gap_2().children(build_children(walk, model, ctx, window, cx)).into_any_element()
}

/// Card — a rounded, softly-elevated panel wrapping its children.
pub(super) fn render_card(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let inner = v_flex().gap_2p5().children(build_children(walk, model, ctx, window, cx));
    div()
        .bg(theme::SURFACE0)
        .border_1()
        .border_color(theme::EDGE)
        .rounded_lg()
        .p_4()
        .w_full()
        .child(inner)
        .into_any_element()
}

/// Modal — render its `trigger` child in-place. When open, the content floats
/// as a top-level overlay (built by [`crate::GpuiApp::render`] after the main
/// tree), so the trigger keeps its place and focus.
pub(super) fn render_modal(
    walk: &Walk,
    _ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    if let Some(trigger_id) = model.get_property::<String>("trigger") {
        render_child(walk, &trigger_id, "", window, cx)
    } else {
        div().into_any_element()
    }
}

/// Tabs — a horizontal tab bar of clickable titles plus the active tab's child
/// panel. The active index comes from the `activeTab` `DynamicNumber`; clicking
/// a tab writes its index back to the binding when bound, else tracks it
/// locally (mirrors the iced/TUI backends).
pub(super) fn render_tabs(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let tabs = read_tabs(model);
    if tabs.is_empty() {
        return div().into_any_element();
    }

    let active_dn = model.get_property::<DynamicNumber>("activeTab");
    let active_path: Option<String> = active_dn.as_ref().and_then(|dn| match dn {
        DynamicNumber::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    });
    let active = match &active_dn {
        Some(dn) => ctx.data_context.resolve_dynamic_number(dn) as usize,
        None => walk
            .local_tabs
            .get(&ctx.component_id)
            .copied()
            .unwrap_or(0),
    }
    .min(tabs.len() - 1);

    let mut bar = TabBar::new(SharedString::from(ctx.component_id.clone())).selected_index(active);
    for (title, _child) in tabs.iter() {
        let title_str = ctx.data_context.resolve_dynamic_string(title);
        bar = bar.child(Tab::new().label(title_str));
    }
    // Write the clicked index back: to the binding when `activeTab` is a data
    // binding, else track it locally.
    let component_id = ctx.component_id.clone();
    let active_path_for_click = active_path.clone();
    bar = bar.on_click(cx.listener(move |this, ix: &usize, _window, cx| {
        if let Some(path) = &active_path_for_click {
            this.set_data_value(path, serde_json::json!(*ix));
        } else {
            this.local_tabs.insert(component_id.clone(), *ix);
        }
        cx.notify();
    }));

    let active_child = tabs[active].1.clone();
    let child_base = ctx.data_context.base_path().to_string();
    let panel = render_child(walk, &active_child, &child_base, window, cx);

    v_flex()
        .gap_0()
        .child(bar)
        .child(Divider::horizontal().color(theme::LINE))
        .child(panel)
        .into_any_element()
}

// ===========================================================================
// Content / leaf
// ===========================================================================

/// Text — styled label; `variant` h1/h2/h3 select heading sizes.
pub(super) fn render_text(ctx: &ComponentContext, model: &ComponentModel) -> AnyElement {
    let content = model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let variant: Option<String> = model.get_property("variant");
    let mut el = div().text_color(theme::TEXT).child(content);
    match variant.as_deref() {
        Some("h1") => el = el.text_size(px(28.)),
        Some("h2") => el = el.text_size(px(22.)),
        Some("h3") => el = el.text_size(px(18.)),
        _ => {}
    }
    el.into_any_element()
}

/// Divider — a faint horizontal rule matching the dark palette.
pub(super) fn render_divider() -> AnyElement {
    Divider::horizontal().color(theme::LINE).into_any_element()
}

/// Icon — maps an icon name to an emoji / unicode glyph (mirrors the TUI/iced
/// backends' `map_icon`); unknown names fall back to the first two chars.
pub(super) fn render_icon(ctx: &ComponentContext, model: &ComponentModel) -> AnyElement {
    let name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    div()
        .text_color(theme::ACCENT)
        .text_size(px(18.))
        .child(map_icon(&name))
        .into_any_element()
}

/// DateTimeInput — a native, editable ISO date/time field bound to a styled
/// `Input` (reusing the TextField chrome), like the iced backend. The format
/// hint is shown as the placeholder.
pub(super) fn render_date_time_input(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let enable_date: bool = model.get_property("enableDate").unwrap_or(true);
    let enable_time: bool = model.get_property("enableTime").unwrap_or(true);
    let hint: SharedString = match (enable_date, enable_time) {
        (true, true) => "YYYY-MM-DDTHH:MM:SS".into(),
        (true, false) => "YYYY-MM-DD".into(),
        (false, true) => "HH:MM:SS".into(),
        (false, false) => "ISO datetime".into(),
    };
    render_labeled_input(walk, ctx, model, &label, hint, window, cx)
}

/// Image — renders a real decoded raster image. Every source (`http(s)` /
/// `data:` / `file://` / local path) is resolved to bytes via the shared
/// `a2ui-image` crate, decoded into a gpui [`Image`], and cached. While a remote
/// fetch is still in flight (or failed) the placeholder chip is shown; the fetch
/// is spawned on gpui's background executor and updates the cache + repaints.
pub(super) fn render_image(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let description = model
        .get_property::<DynamicString>("description")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let fit: Option<String> = model.get_property("fit");
    let content_fit = map_content_fit(fit.as_deref());

    // Already decoded → render.
    if let Some(Some(image)) = walk.image_cache.get(&url) {
        return gpui::img(image.clone()).object_fit(content_fit).into_any_element();
    }

    // Not yet fetched and not in flight → spawn a background fetch.
    let should_spawn = !url.is_empty()
        && !walk.image_pending.borrow().contains(&url)
        && !walk.image_cache.contains_key(&url);
    if should_spawn {
        walk.image_pending.borrow_mut().insert(url.clone());
        let url_task = url.clone();
        cx.spawn(async move |view, cx| {
            let url_fetch = url_task.clone();
            let bytes = cx
                .background_executor()
                .spawn(async move { images::resolve_bytes(&url_fetch) })
                .await;
            let _ = view.update(cx, |this, cx| {
                let decoded = bytes.as_ref().and_then(|b| images::decode_image(&url_task, b));
                this.image_cache
                    .insert(url_task.clone(), decoded.map(Arc::new));
                this.image_pending.borrow_mut().remove(&url_task);
                cx.notify();
            });
        })
        .detach();
    }

    // Placeholder: empty / not-yet-loaded / failed.
    let label = if description.is_empty() { "image" } else { description.as_str() };
    chip("🖼", &format!("image · {label}"))
}

/// Video / AudioPlayer — a chip badge. GPUI ships no media playback widget, so
/// these stay placeholders (mirrors iced).
pub(super) fn render_media_placeholder(
    kind: &str,
    ctx: &ComponentContext,
    model: &ComponentModel,
) -> AnyElement {
    let url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let glyph = match kind {
        "Video" => "▷",
        "Audio" => "♪",
        _ => "◆",
    };
    chip(glyph, &format!("{kind} · {url}"))
}

// ===========================================================================
// Interactive (native gpui-component widgets)
// ===========================================================================

/// Button — labeled press target. A press dispatches `Enter` to its component
/// via the core pipeline (reuses `GpuiApp::handle_activate`), like the other
/// backends' hosts. The label is the Button's single `child` (a Text).
pub(super) fn render_button(
    ctx: &ComponentContext,
    model: &ComponentModel,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let label = resolve_child_text(ctx, model).unwrap_or_else(|| {
        model
            .accessibility()
            .and_then(|a| a.label)
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
            .unwrap_or_default()
    });
    let variant: Option<String> = model.get_property("variant");
    let checks_pass = evaluate_checks(ctx, model);

    let mut btn = Button::new(SharedString::from(ctx.component_id.clone())).label(label);
    btn = match variant.as_deref() {
        Some("primary") => btn.primary(),
        // The A2UI `borderless` variant maps onto gpui-component's transparent
        // `ghost` style (no `borderless` exists).
        Some("borderless") => btn.ghost(),
        // `secondary` is gpui-component's default Button style — no method to
        // call, leave the builder untouched.
        _ => btn,
    };
    if !checks_pass {
        btn = btn.disabled(true);
    }
    let id = ctx.component_id.clone();
    btn.on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
        this.handle_activate(&id);
        cx.notify();
    }))
    .into_any_element()
}

/// TextField — gpui-component `Input` bridged to the data model. The value is
/// resolved from the model once (to seed the persistent [`InputState`]); edits
/// emit back through the subscription captured at lazy-create time.
pub(super) fn render_text_field(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    render_labeled_input(walk, ctx, model, &label, SharedString::from(label.clone()), window, cx)
}

/// Shared body for TextField + DateTimeInput: resolve the `value` binding,
/// lazily create + subscribe the [`InputState`], render a label above an
/// `Input`. `placeholder` is the field's placeholder text.
fn render_labeled_input(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    label: &str,
    placeholder: SharedString,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let value_binding = model.get_property::<DynamicString>("value");
    let resolved = value_binding
        .as_ref()
        .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
        .unwrap_or_default();
    let id = ctx.component_id.clone();

    let state = ensure_input_state(walk, ctx, &id, &resolved, placeholder, value_binding, window, cx);

    let mut col = v_flex().gap_1p5().w_full();
    if !label.is_empty() {
        col = col.child(
            div()
                .text_color(theme::SUBTEXT0)
                .text_size(px(12.))
                .child(SharedString::from(label.to_string())),
        );
    }
    col = col.child(Input::new(&state));
    col.into_any_element()
}

/// Lazily create (or fetch) the [`InputState`] for component `id`, seeding its
/// value from `resolved` and subscribing for write-back to `value_binding`'s
/// path. The entity is cached in [`Walk::text_states`]; the subscription lives
/// in [`Walk::subscriptions`].
#[allow(clippy::too_many_arguments)]
fn ensure_input_state(
    walk: &Walk,
    ctx: &ComponentContext,
    id: &str,
    resolved: &str,
    placeholder: SharedString,
    value_binding: Option<DynamicString>,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> Entity<InputState> {
    if let Some(existing) = walk.text_states.borrow().get(id).cloned() {
        return existing;
    }

    let binding_path: Option<String> = match &value_binding {
        Some(DynamicString::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };
    let initial = resolved.to_string();
    let new_state = cx.new(|cx| {
        let mut s = InputState::new(window, cx).placeholder(placeholder);
        if !initial.is_empty() {
            s.set_value(initial.clone(), window, cx);
        }
        s
    });

    // Write back to the data model on every change (only when bound).
    let sub = cx.subscribe(
        &new_state,
        move |this: &mut GpuiApp, src: Entity<InputState>, ev: &InputEvent, cx: &mut Context<GpuiApp>| {
            if let InputEvent::Change = ev
                && let Some(path) = &binding_path
            {
                let val = src.read(cx).value().to_string();
                this.set_data_value(path, Value::String(val));
                cx.notify();
            }
        },
    );
    walk.subscriptions.borrow_mut().push(sub);
    walk.text_states.borrow_mut().insert(id.to_string(), new_state.clone());
    new_state
}

/// CheckBox — gpui-component native checkbox; toggles write back to the data
/// model (the `on_click` handler receives the new checked state).
pub(super) fn render_checkbox(
    ctx: &ComponentContext,
    model: &ComponentModel,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let value_binding = model.get_property::<DynamicBoolean>("value");
    let resolved = value_binding
        .as_ref()
        .map(|db| ctx.data_context.resolve_dynamic_boolean(db))
        .unwrap_or(false);

    let mut cb = Checkbox::new(SharedString::from(ctx.component_id.clone())).checked(resolved).label(label);
    if let Some(DynamicBoolean::Binding(b)) = &value_binding {
        let path = ctx.data_context.resolve_pointer(&b.path);
        cb = cb.on_click(cx.listener(move |this, checked: &bool, _window, cx| {
            this.set_data_value(&path, Value::Bool(*checked));
            cx.notify();
        }));
    }
    cb.into_any_element()
}

/// Slider — gpui-component `Slider` bridged to the data model. The `SliderState`
/// is lazily created (min/max/step/seed from the model) and subscribed for
/// write-back; degenerate ranges widen so the slider never sees an empty span.
pub(super) fn render_slider(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    _window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let value_binding = model.get_property::<DynamicNumber>("value");
    let resolved_value = value_binding
        .as_ref()
        .map(|dn| ctx.data_context.resolve_dynamic_number(dn))
        .unwrap_or(0.0);
    let min = model
        .get_property::<DynamicNumber>("min")
        .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
        .unwrap_or(0.0);
    let max = model
        .get_property::<DynamicNumber>("max")
        .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
        .unwrap_or(100.0);
    let steps = model
        .get_property::<DynamicNumber>("steps")
        .map(|dn| ctx.data_context.resolve_dynamic_number(&dn));
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();

    let (range_min, range_max, clamped) = resolve_slider_range_and_value(min, max, resolved_value);
    let state = ensure_slider_state(
        walk,
        &ctx.component_id,
        range_min,
        range_max,
        steps,
        clamped,
        value_binding,
        ctx,
        cx,
    );

    let mut col = v_flex().gap_1p5().w_full();
    if !label.is_empty() {
        col = col.child(
            div()
                .text_color(theme::SUBTEXT0)
                .text_size(px(12.))
                .child(label),
        );
    }
    col = col.child(Slider::new(&state));
    col.into_any_element()
}

/// Lazily create (or fetch) the [`SliderState`] for `id`, configuring min/max/
/// step and seeding `value`. Subscribed once for write-back to the binding.
#[allow(clippy::too_many_arguments)]
fn ensure_slider_state(
    walk: &Walk,
    id: &str,
    min: f32,
    max: f32,
    steps: Option<f64>,
    value: f32,
    value_binding: Option<DynamicNumber>,
    ctx: &ComponentContext,
    cx: &mut Context<GpuiApp>,
) -> Entity<SliderState> {
    if let Some(existing) = walk.slider_states.borrow().get(id).cloned() {
        return existing;
    }

    let binding_path: Option<String> = match &value_binding {
        Some(DynamicNumber::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };

    let step_opt = steps.filter(|&s| s > 0.0).map(|s| s as f32);
    let new_state = cx.new(|_cx| {
        let base = SliderState::new().min(min).max(max);
        let base = if let Some(step) = step_opt { base.step(step) } else { base };
        base.default_value(value)
    });

    let sub = cx.subscribe(
        &new_state,
        move |this: &mut GpuiApp, _src: Entity<SliderState>, ev: &SliderEvent, cx: &mut Context<GpuiApp>| {
            if let SliderEvent::Change(SliderValue::Single(v)) = ev
                && let Some(path) = &binding_path
            {
                this.set_data_value(path, serde_json::json!(*v as f64));
                cx.notify();
            }
        },
    );
    walk.subscriptions.borrow_mut().push(sub);
    walk.slider_states.borrow_mut().insert(id.to_string(), new_state.clone());
    new_state
}

/// ChoicePicker — a list of selectable options.
///
/// - Single selection renders a native [`Select`] dropdown; choosing an option
///   writes back `json!([value])` (an array, matching the TUI/iced backends).
/// - Multiple selection renders a column of native checkboxes; toggling
///   adds/removes the value in the array written back.
///
/// Only a `Binding` `value` is writable; a non-binding value degrades to a
/// read-only control (mirrors the TUI `handle_event` bail-out).
pub(super) fn render_choice_picker(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
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
    let path: Option<String> = match &value_binding {
        Some(DynamicStringList::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };
    let is_multiple = model
        .get_property::<String>("variant")
        .as_deref()
        .map(|v| v == "multipleSelection")
        .unwrap_or(false);

    let mut col = v_flex().gap_1p5().w_full();
    if !label.is_empty() {
        col = col.child(
            div()
                .text_color(theme::SUBTEXT0)
                .text_size(px(12.))
                .child(label),
        );
    }
    if options.is_empty() {
        return col.into_any_element();
    }

    if is_multiple {
        for (opt_label, opt_value) in options {
            let checked = selected_values.contains(&opt_value);
            let cb_id = format!("{}::{}", ctx.component_id, opt_value);
            let mut cb = Checkbox::new(SharedString::from(cb_id)).checked(checked).label(opt_label);
            if let Some(p) = &path {
                let path = p.clone();
                let selected = selected_values.clone();
                let value = opt_value.clone();
                cb = cb.on_click(cx.listener(move |this, now_checked: &bool, _window, cx| {
                    let mut next = selected.clone();
                    if *now_checked {
                        if !next.contains(&value) {
                            next.push(value.clone());
                        }
                    } else {
                        next.retain(|v| v != &value);
                    }
                    this.set_data_value(&path, serde_json::json!(next));
                    cx.notify();
                }));
            }
            col = col.child(cb);
        }
    } else {
        let state = ensure_select_state(
            walk,
            &ctx.component_id,
            options,
            selected_values,
            path,
            window,
            cx,
        );
        col = col.child(Select::new(&state).w_full());
    }

    col.into_any_element()
}

/// Lazily create (or fetch) the single-select [`SelectState`] for `id`,
/// populating option labels, the selected index, and subscribing for write-back
/// of `json!([value])` on confirm.
#[allow(clippy::too_many_arguments)]
fn ensure_select_state(
    walk: &Walk,
    id: &str,
    options: Vec<(String, String)>,
    selected_values: Vec<String>,
    path: Option<String>,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> Entity<SelectState<SearchableVec<String>>> {
    if let Some(existing) = walk.select_states.borrow().get(id).cloned() {
        return existing;
    }

    let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
    let selected_index = selected_values.first().and_then(|v| {
        options.iter().position(|(_, val)| val == v).map(IndexPath::new)
    });

    let delegate = SearchableVec::new(labels);
    let new_state = cx.new(|cx| SelectState::new(delegate, selected_index, window, cx));

    let mapping = options.clone();
    let sub = cx.subscribe(
        &new_state,
        move |this: &mut GpuiApp, _src: Entity<SelectState<SearchableVec<String>>>, ev: &SelectEvent<SearchableVec<String>>, cx: &mut Context<GpuiApp>| {
            if let SelectEvent::Confirm(Some(picked)) = ev
                && let Some(path) = &path
            {
                let val = mapping
                    .iter()
                    .find(|(_, v)| v == picked)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                this.set_data_value(path, serde_json::json!([val]));
                cx.notify();
            }
        },
    );
    walk.subscriptions.borrow_mut().push(sub);
    walk.select_states.borrow_mut().insert(id.to_string(), new_state.clone());
    new_state
}

/// Unknown / not-yet-implemented kind — show the kind name + recurse children.
pub(super) fn render_unknown(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let header = chip("?", &format!("{} · unknown", model.component_type));
    let mut col = v_flex().gap_2p5().child(header);
    for child in build_children(walk, model, ctx, window, cx) {
        col = col.child(child);
    }
    col.into_any_element()
}

// ===========================================================================
// Field helpers
// ===========================================================================

/// A small rounded "chip" badge — used to render placeholder components
/// (Icon / Image / Video / AudioPlayer / unknown kinds) so they read as
/// intentional pills rather than bracket text.
fn chip(glyph: &str, label: &str) -> AnyElement {
    h_flex()
        .gap_2()
        .items_center()
        .bg(theme::SURFACE0)
        .border_1()
        .border_color(theme::EDGE)
        .rounded_md()
        .px_3()
        .py_1()
        .child(div().text_color(theme::ACCENT).text_size(px(13.)).child(glyph.to_string()))
        .child(div().text_color(theme::SUBTEXT0).text_size(px(12.)).child(label.to_string()))
        .into_any_element()
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

/// One entry of a Tabs component's `tabs` property: a resolved title plus the
/// child component id to render when this tab is active (mirrors iced).
fn read_tabs(model: &ComponentModel) -> Vec<(DynamicString, String)> {
    let Some(arr) = model.get_raw("tabs").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_tab_entry).collect()
}

/// Parse one `{title, child}` entry of the `tabs` array.
fn parse_tab_entry(v: &Value) -> Option<(DynamicString, String)> {
    let child = v.get("child")?.as_str()?.to_string();
    let title = serde_json::from_value::<DynamicString>(v.get("title")?.clone()).ok()?;
    Some((title, child))
}

/// Read a ChoicePicker's `options` array into `(label, value)` pairs.
fn read_options(model: &ComponentModel) -> Vec<(String, String)> {
    let Some(arr) = model.get_raw("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_choice_option).collect()
}

/// Parse one `{label, value}` option (`value` optional, defaults to empty).
fn parse_choice_option(v: &Value) -> Option<(String, String)> {
    let label = v.get("label")?.as_str()?.to_string();
    let value = v.get("value").and_then(Value::as_str).unwrap_or("").to_string();
    Some((label, value))
}

/// Resolve a ChoicePicker's current selection from its `value` `DynamicStringList`
/// (mirrors iced).
fn resolve_choice_value(ctx: &ComponentContext, dsl: &DynamicStringList) -> Vec<String> {
    match dsl {
        DynamicStringList::Literal(v) => v.clone(),
        DynamicStringList::Binding(b) => match ctx.data_context.get(&b.path) {
            Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            Some(Value::String(s)) => vec![s],
            _ => Vec::new(),
        },
        DynamicStringList::Function(fc) => match ctx
            .data_context
            .resolve_dynamic_value(&DynamicValue::Function(fc.clone()))
        {
            Value::Array(arr) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            Value::String(s) => vec![s],
            _ => Vec::new(),
        },
    }
}

/// Pure helper for [`render_slider`]: clamp + widen the range into `f32`.
fn resolve_slider_range_and_value(min: f64, max: f64, value: f64) -> (f32, f32, f32) {
    let safe_max = if max <= min { min + 1.0 } else { max };
    let clamped = value.clamp(min, safe_max);
    (min as f32, safe_max as f32, clamped as f32)
}

/// Map the A2UI `fit` hint onto GPUI's [`ObjectFit`] (unknown / absent → Contain).
fn map_content_fit(fit: Option<&str>) -> ObjectFit {
    match fit {
        Some("cover") => ObjectFit::Cover,
        Some("fill") => ObjectFit::Fill,
        Some("none") => ObjectFit::None,
        Some("scale-down") => ObjectFit::ScaleDown,
        _ => ObjectFit::Contain,
    }
}

/// Map an A2UI icon name to an emoji / unicode glyph (mirrors the TUI/iced
/// backends' `map_icon`).
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
    fn parse_tab_entry_missing_child_is_skipped() {
        let v = serde_json::json!({ "title": "Overview" });
        assert!(parse_tab_entry(&v).is_none());
    }

    #[test]
    fn parse_choice_option_defaults_value_to_empty() {
        let v = serde_json::json!({ "label": "Code" });
        let (label, value) = parse_choice_option(&v).expect("valid option");
        assert_eq!(label, "Code");
        assert_eq!(value, "");
    }

    #[test]
    fn map_icon_known_and_unknown() {
        assert_eq!(map_icon("mail"), "✉");
        assert_eq!(map_icon("XYZ"), "[XY]");
    }

    #[test]
    fn slider_range_clamps_and_widens() {
        let (lo, hi, v) = resolve_slider_range_and_value(0.0, 100.0, 42.0);
        assert_eq!((lo, hi, v), (0.0, 100.0, 42.0));
        let (lo, hi, v) = resolve_slider_range_and_value(7.0, 7.0, 50.0);
        assert_eq!((lo, hi, v), (7.0, 8.0, 8.0));
    }

    #[test]
    fn content_fit_mapping() {
        assert!(matches!(map_content_fit(Some("cover")), ObjectFit::Cover));
        assert!(matches!(map_content_fit(None), ObjectFit::Contain));
    }
}
