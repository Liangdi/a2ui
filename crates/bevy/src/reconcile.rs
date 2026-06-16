//! The reconciler — diff/patch the A2UI component tree against a stable
//! `component_id → Entity` map, preserving widget entity identity across frames.
//!
//! This is the pattern unique to the Bevy backend. egui rebuilds every frame;
//! Slint rebuilds a flat array every frame. Bevy's interactive widgets
//! (`bevy_ui_widgets` Button/Checkbox/Slider + `bevy_ui_text_input`) only keep
//! correct drag/focus/cursor state when their entities persist, so we:
//!
//! 1. **Plan** (read-only pass over the A2UI model): collect a `Vec<PlanNode>`
//!    describing every component that should exist, its kind, its resolved
//!    fields, its parent, and which root it hangs under (surface vs. overlay).
//! 2. **Apply** (mutating pass over `node_map` + `Commands`): for each planned
//!    node, spawn-if-absent / update-if-present, parent it under its planned
//!    parent, and reorder children. Then despawn any entity in the map that the
//!    plan didn't touch (orphans from a tree change — only when `dirty`).
//!
//! The two-phase split sidesteps the borrow conflict between reading the
//! processor's `RefCell`-backed component map and mutating `node_map` on
//! `A2uiState` — mirroring egui/slint's collected-then-applied shape.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::ecs::hierarchy::ChildOf;
use serde_json::Value as JsonValue;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{DynamicNumber, DynamicString, DynamicStringList};

use crate::render::{NodeFields, build_child_plan, resolve_fields, apply_button, apply_card,
    apply_checkbox, apply_choice_option, apply_column, apply_date_time_input, apply_divider,
    apply_flex_column, apply_icon, apply_image, apply_media_placeholder, apply_modal,
    apply_modal_close, apply_modal_header, apply_modal_panel, apply_modal_scrim, apply_modal_title,
    apply_row, apply_slider, apply_tab_bar, apply_tab_title, apply_tabs, apply_text, apply_text_field};
use crate::state::{A2uiNode, A2uiState, ChoiceOption, ModalDismiss, TabTitle};

/// Which top-level root a planned node hangs under.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Root {
    /// The main surface pane.
    Surface,
    /// A Modal content overlay.
    Overlay,
}

/// Payload carried by a **synthetic** planned node (a tab title / choice option)
/// — interactive chrome the reconciler spawns that is not an A2UI component.
/// `apply_kind` reads it to render the right widget + attach the click-routing
/// marker component. The resolved write-back pointer is captured here at plan
/// time (against the node's base path) so it is correct even inside template
/// children.
#[derive(Clone)]
enum SyntheticMarker {
    /// A tab title button. `active` selects the highlight.
    TabTitle { tabs_id: String, index: usize, active_path: Option<String>, active: bool },
    /// A choice-option button. `selected` selects the highlight.
    ChoiceOption {
        picker_id: String,
        value: String,
        multiple: bool,
        value_path: Option<String>,
        selected: bool,
    },
    /// A Modal dismiss target (scrim backdrop or panel close button). A click
    /// closes the named Modal locally.
    ModalDismiss { modal_id: String },
}

/// One node in the planned tree.
struct PlanNode {
    id: String,
    kind: String,
    parent: Option<String>,
    root: Root,
    fields: NodeFields,
    focused: bool,
    /// For an `Image` node: the decoded `Handle` if it is in the cache this
    /// frame, else `None` (the placeholder renders until `load_images` decodes
    /// it). Resolved in `walk` from `A2uiState::image_cache`.
    image_handle: Option<Handle<Image>>,
    /// Payload for synthetic interactive nodes (`__TabTitle` / `__ChoiceOption`).
    marker: Option<SyntheticMarker>,
}

/// Plan the entire tree: the surface `root` + every open-Modal's `content`.
/// Returns the plan + the set of ids it touched (for orphan cleanup).
fn plan_tree(state: &A2uiState) -> (Vec<PlanNode>, HashSet<String>) {
    let mut nodes = Vec::new();
    let mut touched = HashSet::new();
    let Some(surface) = state.processor.model.surfaces().next() else {
        return (nodes, touched);
    };
    let components = surface.components.borrow();
    let data_model = surface.data_model.borrow();
    let focused_id = state.focus.focused_id().map(str::to_string);

    if components.contains("root") {
        walk(
            "root",
            "",
            None,
            Root::Surface,
            &components,
            &data_model,
            &state.functions,
            focused_id.as_deref(),
            &state.local_tabs,
            &state.image_cache,
            &mut nodes,
            &mut touched,
        );
    }

    // Overlay: each open Modal's `content` subtree.
    let open_modals: Vec<String> = state.open_modals.iter().cloned().collect();
    for modal_id in open_modals {
        let Some(m) = components.get(&modal_id) else { continue };
        if m.component_type != "Modal" { continue; }
        let Some(content_id) = m.get_property::<String>("content") else { continue };
        if !components.contains(&content_id) { continue };
        let title = m
            .get_property::<String>("title")
            .unwrap_or_else(|| "Dialog".to_string());

        // Wrap the Modal's content in overlay chrome (scrim + panel + header),
        // mirroring the Iced/egui backends. The scrim is a full-window dimmed
        // backdrop (click-to-dismiss) that centers the panel; the panel holds a
        // title + close-button header above the content subtree.
        let scrim_id = format!("__a2ui_mscrim:{modal_id}");
        push_synthetic(
            &mut nodes, &mut touched, &scrim_id, None, Root::Overlay, "__ModalScrim",
            NodeFields::empty(),
            Some(SyntheticMarker::ModalDismiss { modal_id: modal_id.clone() }),
        );
        let panel_id = format!("__a2ui_mpanel:{modal_id}");
        push_synthetic(
            &mut nodes, &mut touched, &panel_id, Some(scrim_id.clone()), Root::Overlay,
            "__ModalPanel", NodeFields::empty(), None,
        );
        let header_id = format!("__a2ui_mhdr:{modal_id}");
        push_synthetic(
            &mut nodes, &mut touched, &header_id, Some(panel_id.clone()), Root::Overlay,
            "__ModalHeader", NodeFields::empty(), None,
        );
        let mut title_fields = NodeFields::empty();
        title_fields.text = title;
        let title_node_id = format!("__a2ui_mtitle:{modal_id}");
        push_synthetic(
            &mut nodes, &mut touched, &title_node_id, Some(header_id.clone()), Root::Overlay,
            "__ModalTitle", title_fields, None,
        );
        let mut close_fields = NodeFields::empty();
        close_fields.text = "✕".to_string();
        let close_id = format!("__a2ui_mclose:{modal_id}");
        push_synthetic(
            &mut nodes, &mut touched, &close_id, Some(header_id.clone()), Root::Overlay,
            "__ModalClose", close_fields,
            Some(SyntheticMarker::ModalDismiss { modal_id: modal_id.clone() }),
        );
        // The Modal's content subtree, parented to the panel (after the header).
        walk(
            &content_id,
            "",
            Some(panel_id.clone()),
            Root::Overlay,
            &components,
            &data_model,
            &state.functions,
            focused_id.as_deref(),
            &state.local_tabs,
            &state.image_cache,
            &mut nodes,
            &mut touched,
        );
    }

    (nodes, touched)
}

/// Push one synthetic `PlanNode` (interactive chrome not backed by an A2UI
/// component): a tab bar/title, a choice option/label, or a Modal's overlay
/// scrim/panel/header/title/close. Mirrors what `walk` does for a real node,
/// minus the property resolution (the caller fills `fields`).
#[allow(clippy::too_many_arguments)]
fn push_synthetic(
    nodes: &mut Vec<PlanNode>,
    touched: &mut HashSet<String>,
    id: &str,
    parent: Option<String>,
    root: Root,
    kind: &str,
    fields: NodeFields,
    marker: Option<SyntheticMarker>,
) {
    nodes.push(PlanNode {
        id: id.to_string(),
        kind: kind.to_string(),
        parent,
        root,
        fields,
        focused: false,
        image_handle: None,
        marker,
    });
    touched.insert(id.to_string());
}

/// Depth-first walk that emits a `PlanNode` for `id` then recurses into its
/// children (honoring the three A2UI child shapes via [`build_child_plan`]).
/// Tabs and ChoicePicker are special-cased (like Modal): they don't use
/// `child`/`children`, so instead of `build_child_plan` the walk emits their
/// synthetic interactive chrome (tab bar + titles / option rows) and — for Tabs
/// — recurses into **only** the active panel. `local_tabs` resolves the active
/// tab for unbound Tabs; `image_cache` resolves decoded `Image` handles.
#[allow(clippy::too_many_arguments)]
fn walk(
    id: &str,
    base_path: &str,
    parent: Option<String>,
    root: Root,
    components: &SurfaceComponentsModel,
    data_model: &DataModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
    local_tabs: &HashMap<String, usize>,
    image_cache: &HashMap<String, Option<Handle<Image>>>,
    nodes: &mut Vec<PlanNode>,
    touched: &mut HashSet<String>,
) {
    let Some(model) = components.get(id) else { return };
    touched.insert(id.to_string());

    let ctx = ComponentContext::new(
        id.to_string(),
        String::new(),
        data_model,
        components,
        functions,
        base_path,
        focused_id.map(|s| s.to_string()),
    );
    let kind = model.component_type.clone();
    let fields = resolve_fields(&kind, &ctx, model);
    let focused = focused_id == Some(id);

    // Resolve a decoded image handle for Image nodes (None while loading /
    // failed → the placeholder renders until `load_images` populates the cache).
    let image_handle = if kind == "Image" {
        image_cache.get(&fields.image_url).and_then(|opt| opt.clone())
    } else {
        None
    };

    nodes.push(PlanNode {
        id: id.to_string(),
        kind: kind.clone(),
        parent: parent.clone(),
        root,
        fields,
        focused,
        image_handle,
        marker: None,
    });

    // Modal: walk the trigger in-tree (content is handled as an overlay above).
    if kind == "Modal" {
        if let Some(trigger_id) = model.get_property::<String>("trigger") {
            walk(&trigger_id, base_path, Some(id.to_string()), root,
                components, data_model, functions, focused_id, local_tabs, image_cache,
                nodes, touched);
        }
        return;
    }

    // Tabs: emit the tab bar (Row) + clickable titles, then walk ONLY the
    // active panel's child. Tabs carries no `child`/`children`.
    if kind == "Tabs" {
        let tabs = read_tabs(model);
        if tabs.is_empty() {
            return;
        }
        let active_dn = model.get_property::<DynamicNumber>("activeTab");
        let active_path = active_dn.as_ref().and_then(|dn| match dn {
            DynamicNumber::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
            _ => None,
        });
        let active = match &active_dn {
            Some(dn) => ctx.data_context.resolve_dynamic_number(dn) as usize,
            None => local_tabs.get(id).copied().unwrap_or(0),
        }
        .min(tabs.len() - 1);

        // Synthetic tab bar (Row) parented to the Tabs container.
        let bar_id = format!("__a2ui_tabbar:{id}");
        nodes.push(PlanNode {
            id: bar_id.clone(),
            kind: "__TabBar".into(),
            parent: Some(id.to_string()),
            root,
            fields: NodeFields::empty(),
            focused: false,
            image_handle: None,
            marker: None,
        });
        touched.insert(bar_id.clone());

        // One clickable title button per tab.
        for (i, (title_ds, _child)) in tabs.iter().enumerate() {
            let title_id = format!("__a2ui_tab:{id}/{i}");
            let mut tfields = NodeFields::empty();
            tfields.text = ctx.data_context.resolve_dynamic_string(title_ds);
            nodes.push(PlanNode {
                id: title_id.clone(),
                kind: "__TabTitle".into(),
                parent: Some(bar_id.clone()),
                root,
                fields: tfields,
                focused: false,
                image_handle: None,
                marker: Some(SyntheticMarker::TabTitle {
                    tabs_id: id.to_string(),
                    index: i,
                    active_path: active_path.clone(),
                    active: i == active,
                }),
            });
            touched.insert(title_id);
        }

        // The active panel's child, parented to the Tabs container (below the bar).
        let active_child = tabs[active].1.clone();
        let child_base = ctx.data_context.base_path().to_string();
        walk(&active_child, &child_base, Some(id.to_string()), root,
            components, data_model, functions, focused_id, local_tabs, image_cache,
            nodes, touched);
        return;
    }

    // ChoicePicker: emit an optional label + one clickable row per option.
    // ChoicePicker carries no `child`/`children` — options are a value list.
    if kind == "ChoicePicker" {
        let options = read_options(model);
        let value_binding = model.get_property::<DynamicStringList>("value");
        let selected = value_binding
            .as_ref()
            .map(|dsl| resolve_choice_value(&ctx, dsl))
            .unwrap_or_default();
        let value_path = match &value_binding {
            Some(DynamicStringList::Binding(b)) => {
                Some(ctx.data_context.resolve_pointer(&b.path))
            }
            _ => None,
        };
        let multiple = model
            .get_property::<String>("variant")
            .as_deref()
            .map(|v| v == "multipleSelection")
            .unwrap_or(false);

        // Optional label row (ChoicePicker may carry a `label` property).
        if !ctx_looks_like_empty_label(model, &ctx) {
            let label_id = format!("__a2ui_choicelabel:{id}");
            let mut lfields = NodeFields::empty();
            lfields.text = model
                .get_property::<DynamicString>("label")
                .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
                .unwrap_or_default();
            nodes.push(PlanNode {
                id: label_id.clone(),
                kind: "__ChoiceLabel".into(),
                parent: Some(id.to_string()),
                root,
                fields: lfields,
                focused: false,
                image_handle: None,
                marker: None,
            });
            touched.insert(label_id);
        }

        for (i, (opt_label, opt_value)) in options.iter().enumerate() {
            let opt_id = format!("__a2ui_choice:{id}/{i}");
            let is_sel = selected.iter().any(|s| s == opt_value);
            let prefix = match (multiple, is_sel) {
                (true, true) => "☑ ",
                (true, false) => "☐ ",
                (false, true) => "● ",
                (false, false) => "○ ",
            };
            let mut ofields = NodeFields::empty();
            ofields.text = format!("{prefix}{opt_label}");
            nodes.push(PlanNode {
                id: opt_id.clone(),
                kind: "__ChoiceOption".into(),
                parent: Some(id.to_string()),
                root,
                fields: ofields,
                focused: false,
                image_handle: None,
                marker: Some(SyntheticMarker::ChoiceOption {
                    picker_id: id.to_string(),
                    value: opt_value.clone(),
                    multiple,
                    value_path: value_path.clone(),
                    selected: is_sel,
                }),
            });
            touched.insert(opt_id);
        }
        return;
    }

    for (child_id, child_base) in build_child_plan(model, &ctx) {
        walk(&child_id, &child_base, Some(id.to_string()), root,
            components, data_model, functions, focused_id, local_tabs, image_cache,
            nodes, touched);
    }
}

/// Read a Tabs component's `tabs` property into `(title, child_id)` pairs.
/// Ported from the Iced backend's `read_tabs` (itself from the TUI reference).
fn read_tabs(model: &a2ui_base::model::component_model::ComponentModel) -> Vec<(DynamicString, String)> {
    let Some(arr) = model.get_raw("tabs").and_then(JsonValue::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_tab_entry).collect()
}

/// Parse one `{title, child}` entry of the `tabs` array.
fn parse_tab_entry(v: &JsonValue) -> Option<(DynamicString, String)> {
    let child = v.get("child")?.as_str()?.to_string();
    let title = serde_json::from_value::<DynamicString>(v.get("title")?.clone()).ok()?;
    Some((title, child))
}

/// Read a ChoicePicker's `options` property into `(label, value)` pairs.
/// Ported from the Iced backend's `read_options`.
fn read_options(
    model: &a2ui_base::model::component_model::ComponentModel,
) -> Vec<(String, String)> {
    let Some(arr) = model.get_raw("options").and_then(JsonValue::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_choice_option).collect()
}

/// Parse one `{label, value}` option (`value` defaults to empty).
fn parse_choice_option(v: &JsonValue) -> Option<(String, String)> {
    let label = v.get("label")?.as_str()?.to_string();
    let value = v
        .get("value")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .to_string();
    Some((label, value))
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
        let v = serde_json::json!({ "title": { "path": "/title" }, "child": "c1" });
        let (title, child) = parse_tab_entry(&v).expect("valid entry");
        assert_eq!(child, "c1");
        assert!(matches!(title, DynamicString::Binding(_)));
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
}

/// Resolve a ChoicePicker's selection as `Vec<String>` from its `value`
/// `DynamicStringList`. Ported from the Iced backend's `resolve_choice_value`.
fn resolve_choice_value(
    ctx: &ComponentContext,
    dsl: &DynamicStringList,
) -> Vec<String> {
    use a2ui_base::protocol::common_types::DynamicValue;
    match dsl {
        DynamicStringList::Literal(v) => v.clone(),
        DynamicStringList::Binding(b) => match ctx.data_context.get(&b.path) {
            Some(JsonValue::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(JsonValue::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        },
        DynamicStringList::Function(fc) => {
            match ctx
                .data_context
                .resolve_dynamic_value(&DynamicValue::Function(fc.clone()))
            {
                JsonValue::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                JsonValue::String(s) => vec![s],
                _ => Vec::new(),
            }
        }
    }
}

/// Whether a ChoicePicker has a non-empty `label` (decides whether to emit the
/// synthetic label row). Mirrors the Iced backend's `!label.is_empty()` guard.
fn ctx_looks_like_empty_label(
    model: &a2ui_base::model::component_model::ComponentModel,
    ctx: &ComponentContext,
) -> bool {
    model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default()
        .is_empty()
}

/// The reconciler system. Runs every frame: plan (read), then apply (mutate).
pub fn reconcile(mut state: NonSendMut<A2uiState>, mut commands: Commands) {
    let (plan, touched) = plan_tree(&state);

    // Resolve the two top-level roots (created lazily by the plugin; if absent,
    // we can't parent anything — bail).
    let surface_root = match state.surface_root {
        Some(e) => e,
        None => return,
    };
    let overlay_root = state.overlay_root;

    // First touch: ensure every planned node has an entity, parented correctly,
    // with its kind applied + fields updated. We collect child-order per parent.
    let mut parent_children: HashMap<String, Vec<Entity>> = HashMap::new();
    let mut overlay_children: Vec<Entity> = Vec::new();

    for node in &plan {
        let entry = state.node_map.get(node.id.as_str()).copied();
        let prev_kind = state.kind_map.get(node.id.as_str()).cloned();
        // If the same id now maps to a *different* component_type, the existing
        // entity carries stale components from the old type (e.g. a Text root in
        // one sample vs. a Row root in another). Despawn + respawn so the new
        // type applies cleanly — otherwise both render, visually stacked.
        let kind_changed = matches!(&prev_kind, Some(k) if k != &node.kind);
        let is_new = entry.is_none() || kind_changed;
        if let (Some(e), true) = (entry, kind_changed) {
            commands.entity(e).despawn();
            state.node_map.remove(&node.id);
            state.kind_map.remove(&node.id);
        }
        let entity = match state.node_map.get(node.id.as_str()).copied() {
            Some(e) => e,
            None => {
                // Spawn with the marker; the kind components are applied below
                // via the same EntityCommands path so spawn + update are uniform.
                let eid = commands
                    .spawn((
                        A2uiNode {
                            id: node.id.clone(),
                            kind: node.kind.clone(),
                            overlay: node.root == Root::Overlay,
                        },
                        Node::default(),
                    ))
                    .id();
                state.node_map.insert(node.id.clone(), eid);
                state.kind_map.insert(node.id.clone(), node.kind.clone());
                eid
            }
        };

        // Apply kind + fields to the entity (idempotent). `icon_font` is the
        // embedded emoji font Icons draw in; it is loaded in `setup_base_ui`
        // (Startup), so it is always Some by the time the reconciler (Update)
        // runs.
        apply_kind(commands.entity(entity), node, state.icon_font.as_ref());

        // On first spawn of a TextField / DateTimeInput, seed its buffer with the
        // resolved **value** via the widget's own action queue (applied by its
        // system). Both keep their editable content under the `value` property
        // (not `text`), so this reads `fields.value_string`.
        if is_new
            && (node.kind == "TextField" || node.kind == "DateTimeInput")
            && !node.fields.value_string.is_empty()
        {
            let text = node.fields.value_string.clone();
            commands.entity(entity).queue(move |mut entity: EntityWorldMut| {
                if let Some(mut q) = entity.get_mut::<bevy_ui_text_input::TextInputQueue>() {
                    // `Paste` (unit variant) reads the clipboard; to set text
                    // directly we use an `Edit::Paste(String)` action.
                    q.add(bevy_ui_text_input::actions::TextInputAction::Edit(
                        bevy_ui_text_input::actions::TextInputEdit::Paste(text),
                    ));
                }
            });
        }

        // Track child ordering.
        match &node.parent {
            Some(p) => {
                parent_children.entry(p.clone()).or_default().push(entity);
            }
            None => {
                if node.root == Root::Overlay {
                    overlay_children.push(entity);
                } else {
                    parent_children
                        .entry("__surface_root__".to_string())
                        .or_default()
                        .push(entity);
                }
            }
        }
    }

    // Parent every entity under its planned parent + set root-level children.
    // (Set the ChildOf component; Bevy's hierarchy system resolves the rest.)
    for node in &plan {
        let Some(&entity) = state.node_map.get(node.id.as_str()) else {
            continue;
        };
        let parent_entity = match &node.parent {
            Some(p) => state.node_map.get(p.as_str()).copied(),
            None => {
                if node.root == Root::Overlay {
                    overlay_root
                } else {
                    Some(surface_root)
                }
            }
        };
        if let Some(parent_e) = parent_entity {
            commands.entity(entity).insert(ChildOf(parent_e));
        }
    }

    // Reorder: clear + re-add children in planned order. For v1 we rely on
    // insert-order of ChildOf; a full reorder pass is a future refinement.

    // Orphan cleanup: when the tree structure changed, despawn entities that
    // are in the map but weren't touched this frame.
    if state.dirty {
        let orphans: Vec<(String, Entity)> = state
            .node_map
            .iter()
            .filter(|(id, _)| !touched.contains(id.as_str()))
            .map(|(id, &e)| (id.clone(), e))
            .collect();
        for (id, entity) in orphans {
            commands.entity(entity).despawn();
            state.node_map.remove(&id);
            state.kind_map.remove(&id);
        }
        state.dirty = false;
    }
}

/// Apply the component kind + resolved fields to an entity (spawn or update).
/// Consumes the `EntityCommands` — each node dispatches to exactly one arm.
/// `icon_font` is the embedded emoji font Icons draw in (None only before the
/// plugin's Startup system runs).
fn apply_kind(mut cmd: EntityCommands, node: &PlanNode, icon_font: Option<&Handle<Font>>) {
    match node.kind.as_str() {
        "Column" | "List" => apply_column(cmd),
        "Row" => apply_row(cmd),
        "Card" => apply_card(cmd),
        "Tabs" => apply_tabs(cmd),
        "Modal" => apply_modal(cmd),

        "Text" => apply_text(cmd, &node.fields),
        "Divider" => apply_divider(cmd),
        "Icon" => {
            // Icons need the emoji font; fall back to the default font only if
            // the Startup load somehow hasn't run (never in practice).
            if let Some(f) = icon_font {
                apply_icon(cmd, &node.fields, f);
            } else {
                apply_text(cmd, &node.fields);
            }
        }
        "DateTimeInput" => apply_date_time_input(cmd, &node.fields, node.focused),
        // Image is a real decoded raster when its handle is in the cache, else
        // a labeled placeholder (see `apply_image`'s idempotent swap).
        "Image" => apply_image(cmd, &node.fields, node.image_handle.as_ref()),
        "Video" => apply_media_placeholder(cmd, "Video", &node.fields),
        "AudioPlayer" => apply_media_placeholder(cmd, "Audio", &node.fields),
        // ChoicePicker is rendered via synthetic `__ChoiceOption` children
        // (see `walk`); the container itself is a plain column.
        "ChoicePicker" => apply_column(cmd),

        "Button" => apply_button(cmd, &node.fields),
        "TextField" => apply_text_field(cmd, &node.fields, node.focused),
        "CheckBox" => apply_checkbox(cmd, &node.fields),
        "Slider" => apply_slider(cmd, &node.fields),

        // ── Synthetic interactive chrome (not A2UI components) ──────────────
        "__TabBar" => apply_tab_bar(cmd),
        "__TabTitle" => {
            if let Some(SyntheticMarker::TabTitle { tabs_id, index, active_path, active }) =
                &node.marker
            {
                let marker = TabTitle {
                    tabs_id: tabs_id.clone(),
                    index: *index,
                    active_path: active_path.clone(),
                };
                let active = *active;
                let fields = node.fields.clone();
                cmd.insert(marker);
                apply_tab_title(cmd, &fields, active);
            }
        }
        "__ChoiceLabel" => apply_text(cmd, &node.fields),
        "__ChoiceOption" => {
            if let Some(SyntheticMarker::ChoiceOption {
                picker_id, value, multiple, value_path, selected,
            }) = &node.marker
            {
                let marker = ChoiceOption {
                    picker_id: picker_id.clone(),
                    value: value.clone(),
                    multiple: *multiple,
                    value_path: value_path.clone(),
                };
                let selected = *selected;
                let fields = node.fields.clone();
                cmd.insert(marker);
                apply_choice_option(cmd, &fields, selected);
            }
        }

        // ── Synthetic Modal overlay chrome ─────────────────────────────────
        "__ModalScrim" => {
            if let Some(SyntheticMarker::ModalDismiss { modal_id }) = &node.marker {
                cmd.insert(ModalDismiss { modal_id: modal_id.clone() });
            }
            apply_modal_scrim(cmd);
        }
        "__ModalPanel" => apply_modal_panel(cmd),
        "__ModalHeader" => apply_modal_header(cmd),
        "__ModalTitle" => apply_modal_title(cmd, &node.fields),
        "__ModalClose" => {
            if let Some(SyntheticMarker::ModalDismiss { modal_id }) = &node.marker {
                cmd.insert(ModalDismiss { modal_id: modal_id.clone() });
            }
            apply_modal_close(cmd, &node.fields);
        }

        _ => {
            // Unknown — a placeholder label + recurse (children are planned).
            apply_flex_column(cmd);
        }
    }
}
