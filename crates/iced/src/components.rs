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
use std::path::Path;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{
    ChildList, DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList, DynamicValue,
};
use serde_json::Value;

use iced::widget::image;
use iced::widget::{Column, Row};
use iced::widget::{button, checkbox, container, pick_list, rule, slider, text, text_input};
use iced::{ContentFit, Element, Fill};

use crate::message::Message;
use crate::style;
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
    /// Remote-image cache: a resolved URL → its decoded Iced `Handle` once the
    /// background fetch completes (`None` = attempted but failed, so it isn't
    /// refetched). Local-file images bypass this (`Handle::from_path` in
    /// [`render_image`]); only `http(s)` URLs go through the cache.
    pub image_cache: &'a HashMap<String, Option<image::Handle>>,
    /// Locally-tracked active tab index for Tabs components whose `activeTab`
    /// is **not** a data binding (the gallery samples fall here). Keyed by
    /// component id. Bound Tabs write to the model instead and don't use this.
    pub local_tabs: &'a HashMap<String, usize>,
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
        walk.image_cache,
        walk.local_tabs,
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

/// Card — a rounded, softly-elevated panel wrapping its children.
pub(super) fn render_card<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let children = build_children(walk, model, ctx);
    let inner = Column::with_children(children).spacing(10.0);
    container(inner)
        .padding(18.0)
        .width(Fill)
        .style(style::card)
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

/// One entry of a Tabs component's `tabs` property: a resolved title plus the
/// child component id to render when this tab is active. Mirrors the TUI
/// reference (`crates/tui/src/components/tabs.rs::TabEntry`) and the Dioxus /
/// egui backends.
///
/// Built from raw [`Value`]s rather than a `#[derive(Deserialize)]` struct so
/// this crate need not pull in `serde` (only `serde_json`, already a dep): the
/// inner `title` is deserialized as the core `DynamicString`, `child` is a
/// plain component-id string.
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

/// Tabs — a horizontal tab bar of clickable titles plus the active tab's child
/// panel. Unlike the other containers, Tabs does **not** use `child`/`children`;
/// it reads the `tabs` property (`Vec<{title, child}>`), where each `child` is a
/// component id.
///
/// The active index comes from the `activeTab` `DynamicNumber`. Clicking a tab
/// writes its index back to the `activeTab` binding (only when it is a
/// `Binding`; otherwise the bar renders + highlights the active tab but clicks
/// are inert — read-only, mirroring the TUI `handle_event` bail-out). Mirrors
/// the TUI reference and the Dioxus backend.
pub(super) fn render_tabs<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let tabs = read_tabs(model);
    if tabs.is_empty() {
        return text("").into();
    }

    let active_dn = model.get_property::<DynamicNumber>("activeTab");
    // The write-back path, present only when activeTab is a data binding.
    let active_path: Option<String> = active_dn.as_ref().and_then(|dn| match dn {
        DynamicNumber::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    });

    // Resolve the active index: from the model when bound, else from the
    // locally-tracked selection (the gallery samples don't bind `activeTab`,
    // so without local state their tab bar could never switch).
    let active = match &active_dn {
        Some(dn) => ctx.data_context.resolve_dynamic_number(dn) as usize,
        None => walk.local_tabs.get(&ctx.component_id).copied().unwrap_or(0),
    }
    .min(tabs.len() - 1);

    let mut bar = Row::new().spacing(2.0);
    for (i, (title, _child)) in tabs.iter().enumerate() {
        let is_active = i == active;
        // Bound → write the index to the data model; unbound → track it
        // locally. Either way the click switches the active panel.
        let on_press = match &active_path {
            Some(path) => Message::DataUpdate {
                path: path.clone(),
                value: serde_json::json!(i),
            },
            None => Message::TabActivate {
                component_id: ctx.component_id.clone(),
                index: i,
            },
        };
        let title_str = ctx.data_context.resolve_dynamic_string(title);
        let btn = button(text(title_str).size(13.0))
            .style(style::tab(is_active))
            .padding([9.0, 16.0])
            .on_press(on_press);
        bar = bar.push(btn);
    }

    // Render the active tab's child component below the bar.
    let active_child = tabs[active].1.clone();
    let child_base = ctx.data_context.base_path().to_string();
    let panel = render_child(walk, &active_child, &child_base);

    Column::new()
        .spacing(0.0)
        .push(bar)
        .push(rule::horizontal(1.0).style(style::divider))
        .push(panel)
        .into()
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

/// Divider — a faint horizontal rule matching the dark palette.
pub(super) fn render_divider<'a>() -> Element<'a, Message> {
    rule::horizontal(1.0).style(style::divider).into()
}

/// Icon — maps an icon name to an emoji / unicode glyph. Iced's bundled font
/// renders emoji natively, so no icon font is needed; the mapping mirrors the
/// TUI backend's `map_icon` so every renderer agrees on the same symbol set,
/// and unknown names fall back to the first two characters in brackets.
pub(super) fn render_icon<'a>(ctx: &ComponentContext, model: &ComponentModel) -> Element<'a, Message> {
    let name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let glyph = map_icon(&name);
    text(glyph).size(18.0).into()
}

/// Map an A2UI icon name to an emoji / unicode glyph. Mirrors the TUI backend's
/// `map_icon` (`crates/tui/src/components/icon.rs`) and the Dioxus backend's
/// copy for cross-backend parity.
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

/// DateTimeInput — a native, editable ISO date/time field. Iced 0.14 ships no
/// calendar/clock widget, so the value is bound to a styled `text_input`
/// (reusing the TextField chrome): the user types the ISO string and edits
/// write straight back to the data model — a genuinely interactive control
/// (not the read-only label the egui backend shows). `enableDate` /
/// `enableTime` pick the format hint shown as the placeholder:
/// - both   → `YYYY-MM-DDTHH:MM:SS`
/// - date   → `YYYY-MM-DD`
/// - time   → `HH:MM:SS`
/// - neither → the raw ISO hint
///
/// The value is read from the `"value"` property (a `DynamicString`); when it
/// is a `Binding`, edits emit a [`Message::DataUpdate`], mirroring
/// `render_text_field`. A non-binding value is shown read-only.
pub(super) fn render_date_time_input<'a>(
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

    let enable_date: bool = model.get_property("enableDate").unwrap_or(true);
    let enable_time: bool = model.get_property("enableTime").unwrap_or(true);
    let hint = match (enable_date, enable_time) {
        (true, true) => "YYYY-MM-DDTHH:MM:SS",
        (true, false) => "YYYY-MM-DD",
        (false, true) => "HH:MM:SS",
        (false, false) => "ISO datetime",
    };

    let on_change = match &value_binding {
        Some(DynamicString::Binding(b)) => {
            let path = ctx.data_context.resolve_pointer(&b.path);
            Some(move |s: String| Message::DataUpdate {
                path: path.clone(),
                value: Value::String(s),
            })
        }
        _ => None,
    };

    let mut col = Column::new().spacing(6.0);
    if !label.is_empty() {
        col = col.push(text(label.clone()).size(12.0).color(style::SUBTEXT0));
    }
    col = col.push(
        text_input(hint, &resolved)
            .on_input_maybe(on_change)
            .padding([9.0, 12.0])
            .style(style::text_field),
    );
    col.into()
}

/// Image — renders a real decoded raster image (PNG / JPEG / …). Iced's
/// [`image`] widget has no URL fetcher of its own, so:
///
/// - a **local file path** (or `file://` URL) whose file exists is handed
///   straight to `Handle::from_path` — the renderer decodes it lazily;
/// - an **`http(s)` URL** is fetched out-of-band by [`IcedApp`] (see
///   `fetch_sample_images`) and cached as decoded bytes; the matching
///   [`image::Handle`] is looked up in `walk.image_cache`. While it is still
///   downloading (or failed) the placeholder chip is shown.
///
/// Anything else (empty url, `data:` URL, missing file, unsupported scheme)
/// also falls back to the chip. `fit` maps onto Iced `ContentFit` (default
/// `Contain`, matching the Dioxus `object-fit` mapping).
pub(super) fn render_image<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
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

    // Local file → decode immediately (renderer decodes lazily from the path).
    if !url.is_empty() && is_local_url(&url) {
        let path = strip_file_scheme(&url);
        if Path::new(&path).exists() {
            return image(image::Handle::from_path(path))
                .content_fit(content_fit)
                .into();
        }
    }

    // Remote URL → use the async-fetched handle from the cache, if loaded.
    if is_http_url(&url)
        && let Some(Some(handle)) = walk.image_cache.get(&url)
    {
        return image(handle.clone()).content_fit(content_fit).into();
    }

    // Placeholder: empty / unsupported scheme / not-yet-loaded / failed fetch.
    let label = if description.is_empty() { "image" } else { &description };
    chip("🖼", &format!("image · {label}"))
}

/// Video / AudioPlayer — a chip badge. Iced 0.14 ships no media playback
/// widget, and (unlike the Dioxus WebView) cannot play video/audio at all, so
/// these stay placeholders.
pub(super) fn render_media_placeholder<'a>(
    kind: &str, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
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

/// Whether `url` points at a remote resource Iced must fetch out-of-band.
fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Whether `url` is a local reference (a filesystem path, optionally with a
/// `file://` scheme) — i.e. anything that isn't remote or a `data:` URL.
fn is_local_url(url: &str) -> bool {
    !is_http_url(url) && !url.starts_with("data:")
}

/// Strip a leading `file://` scheme so the remainder is a plain filesystem path.
fn strip_file_scheme(url: &str) -> String {
    url.strip_prefix("file://")
        .map(str::to_string)
        .unwrap_or_else(|| url.to_string())
}

/// Map the A2UI `fit` hint onto Iced's [`ContentFit`] (mirrors the Dioxus
/// `object-fit` mapping; unknown / absent → `Contain`).
fn map_content_fit(fit: Option<&str>) -> ContentFit {
    match fit {
        Some("cover") => ContentFit::Cover,
        Some("fill") => ContentFit::Fill,
        Some("none") => ContentFit::None,
        Some("scale-down") => ContentFit::ScaleDown,
        _ => ContentFit::Contain,
    }
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

    let btn = button(text(label)).padding([9.0, 16.0]);
    let btn = match variant.as_deref() {
        Some("primary") => btn.style(style::primary),
        Some("borderless") => btn.style(style::borderless),
        _ => btn.style(style::secondary),
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

    let mut col = Column::new().spacing(6.0);
    if !label.is_empty() {
        col = col.push(
            text(label.clone())
                .size(12.0)
                .color(style::SUBTEXT0),
        );
    }
    col = col.push(
        text_input(&label, &resolved)
            .on_input_maybe(on_change)
            .padding([9.0, 12.0])
            .style(style::text_field),
    );
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
///
/// Reads `min`/`max`/`value`/`steps` from the model (matching the TUI
/// reference): `min` defaults to `0.0`, `max` to `100.0`, and the resolved
/// `value` is clamped into `[min, max]`. A degenerate range (`max <= min`) is
/// widened to `min..=min + 1.0` so Iced never sees an empty span. When `steps`
/// is present and positive the slider snaps via Iced's `.step(...)`.
pub(super) fn render_slider<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
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

    let (range_min, range_max, clamped) =
        resolve_slider_range_and_value(min, max, resolved_value);
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
    let mut track = slider(range_min..=range_max, clamped, move |v| Message::DataUpdate {
        path: path_opt.clone().unwrap_or_default(),
        value: serde_json::json!(v as f64),
    });
    // Spec's `steps` is the discrete increment between adjacent values; only
    // apply it when present and strictly positive.
    if let Some(step) = steps {
        if step > 0.0 {
            track = track.step(step as f32);
        }
    }

    let mut col = Column::new().spacing(6.0);
    if !label.is_empty() {
        col = col.push(text(label).size(12.0).color(style::SUBTEXT0));
    }
    col = col.push(track);
    col.into()
}

/// Pure helper for [`render_slider`]: given the model-resolved `min`, `max`,
/// and `value` (all `f64`), return the `(min, max, value)` triple — as `f32`
/// for Iced — with the degenerate-range guard and value clamping applied.
///
/// - `max <= min` widens the span to `min..=min + 1.0` (mirrors the TUI
///   `range.abs() < f64::EPSILON` collapse-to-zero handling by ensuring a
///   valid, non-empty track rather than dividing by zero).
/// - the resolved `value` is clamped into `[min, max]`.
fn resolve_slider_range_and_value(min: f64, max: f64, value: f64) -> (f32, f32, f32) {
    let safe_max = if max <= min { min + 1.0 } else { max };
    let clamped = value.clamp(min, safe_max);
    (min as f32, safe_max as f32, clamped as f32)
}

/// An option entry in a ChoicePicker: the display label plus the value written
/// back when chosen. Mirrors the TUI/egui/Dioxus backends' `ChoiceOption`.
/// Built from raw [`Value`]s (no `serde` derive — see [`read_tabs`]).
fn read_options(model: &ComponentModel) -> Vec<(String, String)> {
    let Some(arr) = model.get_raw("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_choice_option).collect()
}

/// Parse one `{label, value}` option of the `options` array. `value` is
/// optional in the spec (defaults to an empty string, matching the
/// `#[serde(default)]` on the TUI/Dioxus `ChoiceOption`); an entry missing a
/// label is skipped.
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
/// string in the data model (mirroring the TUI reference).
fn resolve_choice_value(ctx: &ComponentContext, dsl: &DynamicStringList) -> Vec<String> {
    match dsl {
        DynamicStringList::Literal(v) => v.clone(),
        DynamicStringList::Binding(b) => match ctx.data_context.get(&b.path) {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(Value::String(s)) => vec![s],
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

/// ChoicePicker — a list of selectable options.
///
/// - Single selection (`variant == "mutuallyExclusive"` or default) renders a
///   native [`pick_list`] dropdown; choosing an option writes back
///   `json!([value])` (an array, matching the TUI backend's `EventResult`).
/// - Multiple selection (`variant == "multipleSelection"`) renders a column of
///   native checkboxes; toggling adds/removes the value in the array written
///   back.
///
/// Only a `Binding` `value` is writable; a `Literal`/`Function`/absent value
/// degrades to a read-only control (single: a no-op `pick_list`; multi:
/// checkboxes with no `on_toggle`), matching how the TUI `handle_event` bails
/// on non-binding values.
pub(super) fn render_choice_picker<'a>(
    ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
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

    let is_multiple = model
        .get_property::<String>("variant")
        .as_deref()
        .map(|v| v == "multipleSelection")
        .unwrap_or(false);

    let mut col = Column::new().spacing(6.0).width(Fill);
    if !label.is_empty() {
        col = col.push(text(label).size(12.0).color(style::SUBTEXT0));
    }

    if options.is_empty() {
        return col.into();
    }

    if is_multiple {
        // Multiple selection — a column of native checkboxes. Each toggle
        // recomputes the selection array from the value captured at view time
        // (view runs fresh each frame, so the captured set is current).
        for (opt_label, opt_value) in options {
            let checked = selected_values.contains(&opt_value);
            let cb = match &path {
                Some(p) => {
                    let path = p.clone();
                    let selected = selected_values.clone();
                    let value = opt_value.clone();
                    checkbox(checked)
                        .label(opt_label)
                        .on_toggle(move |now_checked: bool| {
                            let mut next = selected.clone();
                            if now_checked {
                                if !next.contains(&value) {
                                    next.push(value.clone());
                                }
                            } else {
                                next.retain(|v| v != &value);
                            }
                            Message::DataUpdate {
                                path: path.clone(),
                                value: serde_json::json!(next),
                            }
                        })
                }
                None => checkbox(checked).label(opt_label),
            };
            col = col.push(cb);
        }
    } else {
        // Single selection — a native pick_list dropdown. The list shows option
        // labels and maps the picked label back to its value on select.
        let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
        let selected_label = selected_values.first().and_then(|v| {
            options
                .iter()
                .find(|(_, val)| val == v)
                .map(|(lbl, _)| lbl.clone())
        });

        let mapping = options.clone();
        let path_for_select = path.clone();
        let on_select = move |picked_label: String| {
            // An unbound picker (no path) still needs an on_select (pick_list
            // has no `_maybe` variant); emit an empty-path DataUpdate that
            // `update` ignores, mirroring `render_slider`.
            let Some(path) = path_for_select.as_ref() else {
                return Message::DataUpdate {
                    path: String::new(),
                    value: Value::Null,
                };
            };
            let val = mapping
                .iter()
                .find(|(l, _)| l == &picked_label)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            Message::DataUpdate {
                path: path.clone(),
                value: serde_json::json!([val]),
            }
        };

        let pick = pick_list(labels, selected_label, on_select)
            .padding([9.0, 12.0])
            .width(Fill)
            .style(style::pick_list);
        col = col.push(pick);
    }

    col.into()
}

/// Unknown / not-yet-implemented kind — show the kind name + recurse children.
pub(super) fn render_unknown<'a>(
    walk: &Walk<'_>, ctx: &ComponentContext, model: &ComponentModel,
) -> Element<'a, Message> {
    let header = chip("?", &format!("{} · unknown", model.component_type));
    let mut col = Column::new().spacing(10.0).push(header);
    for child in build_children(walk, model, ctx) {
        col = col.push(child);
    }
    col.into()
}

// ===========================================================================
// Field helpers
// ===========================================================================

/// A small rounded "chip" badge — used to render placeholder components
/// (Icon / Image / Video / AudioPlayer / ChoicePicker / unknown kinds) so they
/// read as intentional pills rather than bracket text.
fn chip<'a>(glyph: &str, label: &str) -> Element<'a, Message> {
    let row = Row::new()
        .spacing(8.0)
        .align_y(iced::alignment::Vertical::Center)
        .push(text(glyph.to_string()).color(style::ACCENT).size(13.0))
        .push(text(label.to_string()).color(style::SUBTEXT0).size(12.0));
    container(row).style(style::chip).padding([6.0, 12.0]).into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tab_entry_literal_title() {
        let v = serde_json::json!({ "title": "Overview", "child": "overview-col" });
        let (title, child) = parse_tab_entry(&v).expect("valid entry");
        assert_eq!(child, "overview-col");
        assert_eq!(
            title,
            DynamicString::Literal("Overview".to_string())
        );
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
    fn map_icon_known_name() {
        assert_eq!(map_icon("mail"), "✉");
        assert_eq!(map_icon("star"), "★");
        assert_eq!(map_icon("settings"), "⚙");
    }

    #[test]
    fn map_icon_unknown_falls_back_to_bracketed_prefix() {
        // Unknown names take the first two chars in brackets.
        assert_eq!(map_icon("XYZ"), "[XY]");
        assert_eq!(map_icon("k"), "[k]");
    }

    #[test]
    fn slider_range_clamps_value_into_bounds() {
        // A value inside the range is passed through unchanged.
        let (lo, hi, v) = resolve_slider_range_and_value(0.0, 100.0, 42.0);
        assert_eq!((lo, hi, v), (0.0, 100.0, 42.0));
    }

    #[test]
    fn slider_range_clamps_over_max() {
        let (_, hi, v) = resolve_slider_range_and_value(10.0, 20.0, 99.0);
        assert_eq!((hi, v), (20.0, 20.0));
    }

    #[test]
    fn slider_range_clamps_under_min() {
        let (lo, _, v) = resolve_slider_range_and_value(10.0, 20.0, -5.0);
        assert_eq!((lo, v), (10.0, 10.0));
    }

    #[test]
    fn slider_range_widens_degenerate_span() {
        // max <= min: the span widens to min..=min+1 so Iced sees a valid
        // non-empty range; an out-of-bounds value then clamps to the new max.
        let (lo, hi, v) = resolve_slider_range_and_value(7.0, 7.0, 50.0);
        assert_eq!((lo, hi, v), (7.0, 8.0, 8.0));
        // Inverted range (max < min) is treated the same way.
        let (lo, hi, v) = resolve_slider_range_and_value(7.0, 3.0, 50.0);
        assert_eq!((lo, hi, v), (7.0, 8.0, 8.0));
        // A value below min still clamps to min under the widened range.
        let (lo, _, v) = resolve_slider_range_and_value(7.0, 7.0, 0.0);
        assert_eq!((lo, v), (7.0, 7.0));
    }
}
