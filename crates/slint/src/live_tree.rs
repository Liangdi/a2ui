//! Flat A2UI component map → Slint reactive node array.
//!
//! [`build_nodes`] walks a [`SurfaceModel`]'s component tree (root first),
//! resolving every dynamic value to a concrete string/bool/number, and returns a
//! **flat** `Vec<LiveNode>` where each node's `children` is a list of **indices**
//! into that same array (root is at index 0).
//!
//! ## Why flat + indices (not a nested tree)
//!
//! Slint can't express recursion: a `.slint` struct cannot contain itself
//! (slint-ui/slint#4218), and a component cannot reference itself. So instead of
//! a recursive `LiveNode { children: [LiveNode] }`, we emit a flat array and let
//! the generated `Node0..NodeN` component chain in `build.rs` render it: `NodeK`
//! reads `nodes[idx]`, and for each child index instantiates `Node{K-1}`. This
//! gives bounded-depth (configurable in `build.rs`) rendering of arbitrary A2UI
//! trees without any recursion at the Slint level.
//!
//! Whenever the surface mutates, re-call [`build_nodes`] and push the result into
//! the `Surface` component's `nodes` property — Slint redraws reactively.

use std::collections::{HashMap, HashSet};

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::model::surface_model::SurfaceModel;
use a2ui_base::protocol::common_types::{
    ChildList, DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList,
};
use serde_json::Value;

use crate::ui::{ChoiceOption, LiveNode};

/// Build the flat node array for a surface's `"root"` component.
///
/// Returns an empty `Vec` when the surface has no `root`. Index 0 is always the
/// root (the `Surface` component renders `Node{MAX_DEPTH}` at index 0).
///
/// `image_cache` supplies decoded rasters for `Image` nodes (url → `slint::Image`),
/// produced out-of-band by the host; a url absent from the cache degrades to the
/// labeled placeholder.
pub fn build_nodes(
    surface: &SurfaceModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
    open_modal_ids: &HashSet<String>,
    image_cache: &HashMap<String, slint::Image>,
) -> Vec<LiveNode> {
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();
    if !components.contains("root") {
        return Vec::new();
    }
    let mut builder = FlatBuilder {
        nodes: Vec::new(),
        image_cache,
    };
    builder.add(
        "root",
        "",
        &data_model,
        &components,
        functions,
        &surface.id,
        focused_id,
        open_modal_ids,
    );
    builder.nodes
}

/// Build the floating overlay for the first open Modal: a flat node array whose
/// root is that Modal's `content` subtree, for the `Surface` window to render in
/// a top-most overlay layer. Returns an empty `Vec` when no Modal is open.
pub fn build_overlay_nodes(
    surface: &SurfaceModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
    open_modal_ids: &HashSet<String>,
    image_cache: &HashMap<String, slint::Image>,
) -> Vec<LiveNode> {
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();

    // First open Modal's content becomes the overlay root. A Modal counts as
    // open when the host tracks it locally or its `isOpen` is a literal true.
    let content_id = components.all().iter().find_map(|(_, m)| {
        let open = open_modal_ids.contains(m.id.as_str())
            || matches!(
                m.get_property::<DynamicBoolean>("isOpen"),
                Some(DynamicBoolean::Literal(true))
            );
        (m.component_type == "Modal" && open)
            .then(|| m.get_property::<String>("content"))
            .flatten()
    });
    let Some(content_id) = content_id else {
        return Vec::new();
    };

    let mut builder = FlatBuilder {
        nodes: Vec::new(),
        image_cache,
    };
    builder.add(
        &content_id,
        "",
        &data_model,
        &components,
        functions,
        &surface.id,
        focused_id,
        open_modal_ids,
    );
    builder.nodes
}

/// Accumulator that flattens the tree into a `Vec<LiveNode>` with index children.
struct FlatBuilder<'a> {
    nodes: Vec<LiveNode>,
    image_cache: &'a HashMap<String, slint::Image>,
}

impl<'a> FlatBuilder<'a> {
    /// Append a node for `id` (and its subtree). Returns its index in `nodes`,
    /// or `None` if the component doesn't exist.
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        id: &str,
        base_path: &str,
        data_model: &DataModel,
        components: &SurfaceComponentsModel,
        functions: &HashMap<String, Box<dyn FunctionImplementation>>,
        surface_id: &str,
        focused_id: Option<&str>,
        open_modal_ids: &HashSet<String>,
    ) -> Option<usize> {
        let model = components.get(id)?;
        let kind = model.component_type.clone();

        // Resolve this node's display fields while we hold the context borrow,
        // then plan its children — both as fully-owned data so the recursive
        // `add` calls below don't fight the borrow.
        let (resolved, child_plan) = {
            let ctx = ComponentContext::new(
                id.to_string(),
                surface_id.to_string(),
                data_model,
                components,
                functions,
                base_path,
                focused_id.map(|s| s.to_string()),
            );
            let resolved = resolve_fields(&kind, &ctx, model, open_modal_ids, self.image_cache);
            let plan = build_child_plan(model, &ctx);
            (resolved, plan)
        };

        // Reserve this node's slot first so it keeps the lowest index, then
        // recurse into children (they take later indices), then fill the slot.
        let idx = self.nodes.len();
        self.nodes.push(empty_node());
        let mut child_indices: Vec<i32> = Vec::new();
        for (child_id, child_base) in child_plan {
            if let Some(child_idx) = self.add(
                &child_id,
                &child_base,
                data_model,
                components,
                functions,
                surface_id,
                focused_id,
                open_modal_ids,
            ) {
                child_indices.push(child_idx as i32);
            }
        }

        self.nodes[idx] = LiveNode {
            id: id.into(),
            kind: kind.into(),
            text: resolved.text.into(),
            label: resolved.label.into(),
            variant: resolved.variant.into(),
            checked: resolved.checked,
            number: resolved.number as f32,
            min: resolved.min,
            max: resolved.max,
            selected: resolved.selected,
            multiple: resolved.multiple,
            editable: resolved.editable,
            has_image: resolved.has_image,
            extra: resolved.extra.into(),
            choices: to_choice_model(resolved.choices),
            choice_labels: to_string_model(resolved.choice_labels),
            tab_titles: to_string_model(resolved.tab_titles),
            source: resolved.source,
            focused: focused_id == Some(id),
            children: to_int_model(child_indices),
        };
        Some(idx)
    }
}

/// All display fields for a node, ready to copy into a `LiveNode`.
struct Resolved {
    text: String,
    label: String,
    variant: String,
    checked: bool,
    number: f64,
    min: f32,
    max: f32,
    selected: i32,
    multiple: bool,
    editable: bool,
    has_image: bool,
    extra: String,
    choices: Vec<(String, bool)>,
    choice_labels: Vec<String>,
    tab_titles: Vec<String>,
    source: slint::Image,
}

/// A `Resolved` with neutral defaults (no label/value, full slider range, no
/// image, no selection) — the per-kind match overrides only what it needs.
fn blank(variant: String) -> Resolved {
    Resolved {
        text: String::new(),
        label: String::new(),
        variant,
        checked: false,
        number: 0.0,
        min: 0.0,
        max: 100.0,
        selected: -1,
        multiple: false,
        editable: false,
        has_image: false,
        extra: String::new(),
        choices: Vec::new(),
        choice_labels: Vec::new(),
        tab_titles: Vec::new(),
        source: slint::Image::default(),
    }
}

/// Whether a `Modal` component is currently open.
///
/// A Modal is open when its `isOpen` property resolves true **or** its id is in
/// the host's locally-tracked `open_modal_ids` set (the gallery has no server to
/// flip `isOpen`, so trigger interactions are recorded locally instead).
fn modal_is_open(
    model: &a2ui_base::model::component_model::ComponentModel,
    ctx: &ComponentContext,
    open_modal_ids: &HashSet<String>,
) -> bool {
    let prop = model
        .get_property::<DynamicBoolean>("isOpen")
        .map(|db| ctx.data_context.resolve_dynamic_boolean(&db))
        .unwrap_or(false);
    prop || open_modal_ids.contains(&model.id)
}

/// Extract the display fields for a component type.
///
/// Unknown kinds fall through with empty text (the generated `.slint` shows the
/// kind name so an unimplemented component is still visible).
fn resolve_fields(
    kind: &str,
    ctx: &ComponentContext,
    model: &a2ui_base::model::component_model::ComponentModel,
    open_modal_ids: &HashSet<String>,
    image_cache: &HashMap<String, slint::Image>,
) -> Resolved {
    let variant: String = model.get_property::<String>("variant").unwrap_or_default();

    // Helpers closing over the shared context.
    let str_prop = |name: &str| -> String {
        model
            .get_property::<DynamicString>(name)
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
            .unwrap_or_default()
    };
    let num_prop = |name: &str| -> f64 {
        model
            .get_property::<DynamicNumber>(name)
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
            .unwrap_or(0.0)
    };

    match kind {
        "Text" => {
            let mut r = blank(variant);
            r.text = str_prop("text");
            r
        }
        "TextField" => {
            let value_ds = model.get_property::<DynamicString>("value");
            let editable = matches!(&value_ds, Some(DynamicString::Binding(_)));
            let mut r = blank(variant);
            r.label = str_prop("label");
            r.text = value_ds
                .as_ref()
                .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
                .unwrap_or_default();
            r.editable = editable;
            r
        }
        "CheckBox" => {
            let checked = model
                .get_property::<DynamicBoolean>("value")
                .map(|db| ctx.data_context.resolve_dynamic_boolean(&db))
                .unwrap_or(false);
            let mut r = blank(variant);
            r.text = str_prop("label");
            r.checked = checked;
            r
        }
        "Slider" => {
            let number = num_prop("value");
            let min = num_prop("min");
            // Treat a zero max (unset) as the spec default of 100.
            let max_raw = num_prop("max");
            let max = if max_raw <= min { 100.0 } else { max_raw };
            let mut r = blank(variant);
            r.label = str_prop("label");
            r.number = number;
            r.min = min as f32;
            r.max = max as f32;
            r.text = format_number(number);
            r
        }
        "Tabs" => {
            let number = num_prop("activeTab");
            let mut r = blank(variant);
            r.number = number;
            r.tab_titles = read_tab_titles(model, ctx);
            r
        }
        "Icon" => {
            let name = str_prop("name");
            let mut r = blank(variant);
            r.extra = map_icon(&name);
            r
        }
        "DateTimeInput" => {
            let value_ds = model.get_property::<DynamicString>("value");
            let editable = matches!(&value_ds, Some(DynamicString::Binding(_)));
            let enable_date: bool = model.get_property("enableDate").unwrap_or(true);
            let enable_time: bool = model.get_property("enableTime").unwrap_or(true);
            let hint = match (enable_date, enable_time) {
                (true, true) => "YYYY-MM-DDTHH:MM:SS",
                (true, false) => "YYYY-MM-DD",
                (false, true) => "HH:MM:SS",
                (false, false) => "ISO datetime",
            };
            let mut r = blank(variant);
            r.label = str_prop("label");
            r.extra = value_ds
                .as_ref()
                .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
                .unwrap_or_default();
            r.text = hint.to_string();
            r.editable = editable;
            r
        }
        "Image" | "Video" | "AudioPlayer" => {
            let url = str_prop("url");
            let mut r = blank(variant);
            r.extra = url.clone();
            if kind == "Image" {
                if let Some(img) = image_cache.get(&url) {
                    r.has_image = true;
                    r.source = img.clone();
                }
            }
            r
        }
        "ChoicePicker" => {
            let label = str_prop("label");
            let options = read_options(model);
            let multiple = model
                .get_property::<String>("variant")
                .as_deref()
                .map(|v| v == "multipleSelection")
                .unwrap_or(false);
            let selected_values = model
                .get_property::<DynamicStringList>("value")
                .as_ref()
                .map(|dsl| resolve_selected_values(ctx, dsl))
                .unwrap_or_default();

            let mut r = blank(variant);
            r.label = label;
            r.multiple = multiple;
            if multiple {
                r.choices = options
                    .iter()
                    .map(|(lbl, val)| (lbl.clone(), selected_values.contains(val)))
                    .collect();
            } else {
                r.choice_labels = options.iter().map(|(lbl, _)| lbl.clone()).collect();
                r.selected = selected_values
                    .first()
                    .and_then(|v| options.iter().position(|(_, val)| val == v))
                    .map(|i| i as i32)
                    .unwrap_or(-1);
            }
            r
        }
        "Modal" => {
            let mut r = blank(variant);
            r.checked = modal_is_open(model, ctx, open_modal_ids);
            r
        }
        _ => blank(variant),
    }
}

/// Read a ChoicePicker's `options` array as `(label, value)` pairs. `value` is
/// optional in the spec (defaults to empty); an entry missing a label is skipped.
/// Mirrors `crates/iced/src/components.rs::read_options`.
pub(crate) fn read_options(
    model: &a2ui_base::model::component_model::ComponentModel,
) -> Vec<(String, String)> {
    let Some(arr) = model.get_raw("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_choice_option).collect()
}

/// Parse one `{label, value}` option entry.
fn parse_choice_option(v: &Value) -> Option<(String, String)> {
    let label = v.get("label")?.as_str()?.to_string();
    let value = v
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some((label, value))
}

/// Read a Tabs component's `tabs` array, resolving each `title` (literal string
/// or data binding) to its current text. Mirrors `crates/iced/.../read_tabs`.
fn read_tab_titles(
    model: &a2ui_base::model::component_model::ComponentModel,
    ctx: &ComponentContext,
) -> Vec<String> {
    let Some(arr) = model.get_raw("tabs").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(parse_tab_title)
        .map(|t| resolve_title(ctx, t))
        .collect()
}

/// Parse the raw `title` field of a tab entry (string or DynamicString object).
fn parse_tab_title(v: &Value) -> Option<DynamicString> {
    let title = v.get("title")?;
    match title {
        Value::String(s) => Some(DynamicString::Literal(s.clone())),
        _ => serde_json::from_value::<DynamicString>(title.clone()).ok(),
    }
}

/// Resolve a tab `title` DynamicString to its current text.
fn resolve_title(ctx: &ComponentContext, title: DynamicString) -> String {
    ctx.data_context.resolve_dynamic_string(&title)
}

/// Resolve a ChoicePicker's current selection from its `value` DynamicStringList
/// — accepting an array of strings or a single string (mirrors the TUI/iced refs).
fn resolve_selected_values(ctx: &ComponentContext, dsl: &DynamicStringList) -> Vec<String> {
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
        DynamicStringList::Function(_) => Vec::new(),
    }
}

/// Format a slider value for display (integer when whole, one decimal otherwise).
fn format_number(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v:.1}")
    }
}

/// Map an icon name to a Unicode glyph (same table as the TUI reference:
/// `crates/tui/src/components/icon.rs::map_icon`). Unknown names fall back to
/// the first two characters in brackets.
fn map_icon(name: &str) -> String {
    let symbol = match name {
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
        _ => return fallback_icon(name),
    };
    symbol.to_string()
}

/// Fallback for an unknown icon name: first two characters in brackets.
fn fallback_icon(name: &str) -> String {
    let chars: String = name.chars().take(2).collect();
    format!("[{chars}]")
}

/// Plan a node's children as `(child_id, child_base_path)` pairs, honoring all
/// three A2UI child shapes (`child`, static `children`, template `children`).
fn build_child_plan(
    model: &a2ui_base::model::component_model::ComponentModel,
    ctx: &ComponentContext,
) -> Vec<(String, String)> {
    let mut plan = Vec::new();
    let base = ctx.data_context.base_path().to_string();

    // Modal mounts only its trigger in-place; when open the content is shown as
    // a floating overlay (built separately by [`build_overlay_nodes`]), not
    // swapped in here — so the trigger keeps its place (and focus).
    if model.component_type == "Modal" {
        if let Some(trigger_id) = model.get_property::<String>("trigger") {
            plan.push((trigger_id, base));
        }
        return plan;
    }

    // Tabs read its panels from the `tabs` array's `child` fields (not the
    // `children` property). Only the active panel (index == activeTab) is
    // mounted as a child — the generated `.slint` renders whatever is in
    // `me.children`, so excluding the inactive panels hides them from layout
    // (avoids needing an `if` inside the Slint `for`, which the grammar rejects).
    if model.component_type == "Tabs" {
        let active = model
            .get_property::<DynamicNumber>("activeTab")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn) as usize)
            .unwrap_or(0);
        if let Some(arr) = model.get_raw("tabs").and_then(Value::as_array) {
            if active < arr.len() {
                if let Some(child) = arr[active].get("child").and_then(Value::as_str) {
                    plan.push((child.to_string(), base));
                }
            }
        }
        return plan;
    }

    // Single child (Card / Button / wrappers).
    if let Some(child_id) = model.child() {
        plan.push((child_id, base.clone()));
    }

    match model.children() {
        Some(ChildList::Static(ids)) => {
            for cid in ids {
                plan.push((cid, base.clone()));
            }
        }
        Some(ChildList::Template { component_id, path }) => {
            // One instance per element of the bound data array, each at its own
            // nested path (mirrors the tui backend's template expansion).
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

/// Wrap a `Vec<i32>` into the `ModelRc` shape a `[int]` property expects.
fn to_int_model(indices: Vec<i32>) -> slint::ModelRc<i32> {
    slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(indices)))
}

/// Wrap a `Vec<String>` into the `ModelRc` shape a `[string]` property expects.
fn to_string_model(items: Vec<String>) -> slint::ModelRc<slint::SharedString> {
    let v: Vec<slint::SharedString> = items.into_iter().map(Into::into).collect();
    slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(v)))
}

/// Wrap a `Vec<(label, checked)>` into the `ModelRc` shape the `choices`
/// `[ChoiceOption]` property expects.
fn to_choice_model(items: Vec<(String, bool)>) -> slint::ModelRc<ChoiceOption> {
    let v: Vec<ChoiceOption> = items
        .into_iter()
        .map(|(label, checked)| ChoiceOption {
            label: label.into(),
            checked,
        })
        .collect();
    slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(v)))
}

/// A placeholder node used to reserve a slot before its children are recursed.
fn empty_node() -> LiveNode {
    LiveNode {
        id: "".into(),
        kind: "".into(),
        text: "".into(),
        label: "".into(),
        variant: "".into(),
        checked: false,
        number: 0.0,
        min: 0.0,
        max: 100.0,
        selected: -1,
        multiple: false,
        editable: false,
        has_image: false,
        extra: "".into(),
        choices: slint::ModelRc::default(),
        choice_labels: slint::ModelRc::default(),
        tab_titles: slint::ModelRc::default(),
        source: slint::Image::default(),
        focused: false,
        children: slint::ModelRc::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_base::catalog::Catalog;
    use a2ui_base::message_processor::MessageProcessor;
    use slint::Model;

    /// Build a processor seeded with `components_json` (+ optional `/data`).
    fn setup(
        components_json: serde_json::Value,
        data: Option<serde_json::Value>,
    ) -> MessageProcessor {
        let mut processor = MessageProcessor::new(vec![Catalog::new(
            "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
        )]);
        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "test",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "dataModel": data.unwrap_or(serde_json::Value::Object(Default::default())),
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();
        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": { "surfaceId": "test", "components": components_json }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();
        processor
    }

    /// Main node tree for `components_json`'s surface (Modal always shows trigger).
    fn build_open(
        components_json: serde_json::Value,
        data: Option<serde_json::Value>,
        focused_id: Option<&str>,
        open_modal_ids: &HashSet<String>,
    ) -> Vec<LiveNode> {
        let processor = setup(components_json, data);
        let surface = processor.model.get_surface("test").expect("surface exists");
        build_nodes(
            surface,
            &HashMap::new(),
            focused_id,
            open_modal_ids,
            &HashMap::new(),
        )
    }

    /// Overlay node tree (first open Modal's content) for the same surface.
    fn build_overlay_open(
        components_json: serde_json::Value,
        data: Option<serde_json::Value>,
        focused_id: Option<&str>,
        open_modal_ids: &HashSet<String>,
    ) -> Vec<LiveNode> {
        let processor = setup(components_json, data);
        let surface = processor.model.get_surface("test").expect("surface exists");
        build_overlay_nodes(
            surface,
            &HashMap::new(),
            focused_id,
            open_modal_ids,
            &HashMap::new(),
        )
    }

    /// `build` with no locally-open modals (the common case in existing tests).
    fn build(
        components_json: serde_json::Value,
        data: Option<serde_json::Value>,
        focused_id: Option<&str>,
    ) -> Vec<LiveNode> {
        build_open(components_json, data, focused_id, &HashSet::new())
    }

    /// Child indices of `nodes[idx]` as a `Vec<i32>`.
    fn child_ids(nodes: &[LiveNode], idx: usize) -> Vec<i32> {
        nodes[idx].children.iter().collect()
    }

    #[test]
    fn no_root_yields_empty() {
        let mut processor = MessageProcessor::new(vec![Catalog::new("c")]);
        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": { "surfaceId": "s", "catalogId": "c", "dataModel": {} }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();
        let surface = processor.model.get_surface("s").unwrap();
        assert!(
            build_nodes(
                surface,
                &HashMap::new(),
                None,
                &HashSet::new(),
                &HashMap::new()
            )
            .is_empty()
        );
    }

    #[test]
    fn text_root_resolves_to_single_node() {
        let nodes = build(
            serde_json::json!([{ "id": "root", "component": "Text", "text": "Hello" }]),
            None,
            None,
        );
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind.to_string(), "Text");
        assert_eq!(nodes[0].text.to_string(), "Hello");
        assert_eq!(nodes[0].children.iter().count(), 0);
    }

    #[test]
    fn column_children_are_indexed_after_root() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["a", "b"] },
                { "id": "a", "component": "Text", "text": "One" },
                { "id": "b", "component": "Text", "text": "Two" }
            ]),
            None,
            None,
        );
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].kind.to_string(), "Column");
        assert_eq!(child_ids(&nodes, 0), vec![1, 2]);
        assert_eq!(nodes[1].text.to_string(), "One");
        assert_eq!(nodes[2].text.to_string(), "Two");
    }

    #[test]
    fn nested_card_button_uses_indices() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Card", "child": "btn" },
                { "id": "btn", "component": "Button", "child": "lbl" },
                { "id": "lbl", "component": "Text", "text": "Sign In" }
            ]),
            None,
            None,
        );
        assert_eq!(nodes.len(), 3);
        // root(0) → child btn(1); btn(1) → child lbl(2)
        assert_eq!(child_ids(&nodes, 0), vec![1]);
        assert_eq!(child_ids(&nodes, 1), vec![2]);
        assert_eq!(nodes[1].kind.to_string(), "Button");
        assert_eq!(nodes[2].text.to_string(), "Sign In");
    }

    #[test]
    fn template_children_expand_and_resolve_own_path() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": { "path": "/items", "componentId": "item" } },
                { "id": "item", "component": "Text", "text": { "path": "name" } }
            ]),
            Some(serde_json::json!({ "items": [
                { "name": "Alpha" }, { "name": "Beta" }, { "name": "Gamma" }
            ]})),
            None,
        );
        assert_eq!(nodes.len(), 4); // root + 3 instances
        assert_eq!(child_ids(&nodes, 0), vec![1, 2, 3]);
        let texts: Vec<String> = nodes[1..].iter().map(|n| n.text.to_string()).collect();
        assert_eq!(texts, vec!["Alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn focused_flag_marks_only_the_focused_node() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["f", "o"] },
                { "id": "f", "component": "TextField", "label": "L", "value": "" },
                { "id": "o", "component": "TextField", "label": "L2", "value": "" }
            ]),
            None,
            Some("f"),
        );
        assert!(!nodes[0].focused, "root not focused");
        assert!(nodes[1].focused, "first field focused");
        assert!(!nodes[2].focused);
    }

    #[test]
    fn checkbox_carries_checked_from_data_model() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["c"] },
                { "id": "c", "component": "CheckBox", "label": "Agree", "value": { "path": "/flag" } }
            ]),
            Some(serde_json::json!({ "flag": true })),
            None,
        );
        assert_eq!(nodes[1].kind.to_string(), "CheckBox");
        assert_eq!(nodes[1].text.to_string(), "Agree");
        assert!(nodes[1].checked, "checked reflects data-model bool");
    }

    #[test]
    fn slider_carries_number_and_range_from_data_model() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["s"] },
                { "id": "s", "component": "Slider", "value": { "path": "/vol" }, "min": 0, "max": 10 }
            ]),
            Some(serde_json::json!({ "vol": 4 })),
            None,
        );
        assert_eq!(nodes[1].kind.to_string(), "Slider");
        assert_eq!(nodes[1].number, 4.0);
        assert_eq!(nodes[1].min, 0.0);
        assert_eq!(nodes[1].max, 10.0);
    }

    #[test]
    fn choice_picker_single_resolves_labels_and_selected() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["c"] },
                { "id": "c", "component": "ChoicePicker", "label": "Pick",
                  "options": [{"label":"Red","value":"r"},{"label":"Blue","value":"b"}],
                  "value": { "path": "/color" } }
            ]),
            Some(serde_json::json!({ "color": ["b"] })),
            None,
        );
        let n = &nodes[1];
        assert!(!n.multiple);
        let labels: Vec<String> = n.choice_labels.iter().map(|s| s.to_string()).collect();
        assert_eq!(labels, vec!["Red", "Blue"]);
        assert_eq!(n.selected, 1, "Blue is at index 1");
    }

    #[test]
    fn choice_picker_multi_resolves_choices_checked() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["c"] },
                { "id": "c", "component": "ChoicePicker", "variant": "multipleSelection",
                  "options": [{"label":"A","value":"a"},{"label":"B","value":"b"}],
                  "value": { "path": "/sel" } }
            ]),
            Some(serde_json::json!({ "sel": ["a"] })),
            None,
        );
        let n = &nodes[1];
        assert!(n.multiple);
        let choices: Vec<(String, bool)> = n
            .choices
            .iter()
            .map(|o| (o.label.to_string(), o.checked))
            .collect();
        assert_eq!(
            choices,
            vec![("A".to_string(), true), ("B".to_string(), false)]
        );
    }

    #[test]
    fn tabs_mount_panels_from_tabs_array() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Tabs", "activeTab": { "path": "/tab" },
                  "tabs": [ {"title":"One","child":"p1"}, {"title":"Two","child":"p2"} ] },
                { "id": "p1", "component": "Text", "text": "Panel 1" },
                { "id": "p2", "component": "Text", "text": "Panel 2" }
            ]),
            Some(serde_json::json!({ "tab": 1 })),
            None,
        );
        // root + the active panel (p2) only — inactive panels aren't mounted.
        assert_eq!(nodes.len(), 2);
        assert_eq!(child_ids(&nodes, 0), vec![1]);
        assert_eq!(nodes[0].number, 1.0, "activeTab resolved to 1");
        assert_eq!(
            nodes[1].text.to_string(),
            "Panel 2",
            "only the active panel is mounted"
        );
        let titles: Vec<String> = nodes[0].tab_titles.iter().map(|s| s.to_string()).collect();
        assert_eq!(titles, vec!["One", "Two"]);
    }

    #[test]
    fn icon_maps_name_to_glyph() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["i"] },
                { "id": "i", "component": "Icon", "name": "settings" }
            ]),
            None,
            None,
        );
        assert_eq!(nodes[1].extra.to_string(), "⚙");
    }

    #[test]
    fn image_carries_source_in_extra() {
        let nodes = build(
            serde_json::json!([
                { "id": "root", "component": "Column", "children": ["img"] },
                { "id": "img", "component": "Image", "url": "https://example.com/a.png" }
            ]),
            None,
            None,
        );
        assert_eq!(nodes[1].kind.to_string(), "Image");
        assert_eq!(nodes[1].extra.to_string(), "https://example.com/a.png");
        assert!(!nodes[1].has_image, "uncached url stays a placeholder");
    }

    #[test]
    fn modal_mounts_trigger_inplace_and_content_as_overlay() {
        let components = serde_json::json!([
            { "id": "root", "component": "Column", "children": ["m"] },
            { "id": "m", "component": "Modal", "trigger": "open-btn", "content": "body" },
            { "id": "open-btn", "component": "Text", "text": "Open" },
            { "id": "body", "component": "Text", "text": "Content" }
        ]);

        // Main tree always mounts the trigger, open or closed.
        let main_closed = build_open(components.clone(), None, None, &HashSet::new());
        let main_open = build_open(
            components.clone(),
            None,
            None,
            &HashSet::from(["m".to_string()]),
        );
        assert_eq!(main_closed[1].kind.to_string(), "Modal");
        assert_eq!(child_ids(&main_closed, 1), vec![2]);
        assert_eq!(
            main_closed[2].text.to_string(),
            "Open",
            "trigger in main tree when closed"
        );
        assert_eq!(
            main_open[2].text.to_string(),
            "Open",
            "trigger still in main tree when open"
        );

        // Overlay is empty when closed; the content subtree when open.
        assert!(
            build_overlay_open(components.clone(), None, None, &HashSet::new()).is_empty(),
            "no overlay when closed"
        );
        let overlay = build_overlay_open(components, None, None, &HashSet::from(["m".to_string()]));
        assert!(!overlay.is_empty(), "overlay present when open");
        assert_eq!(
            overlay[0].text.to_string(),
            "Content",
            "overlay root is the Modal's content"
        );
    }
}
