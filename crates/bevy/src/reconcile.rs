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

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::ecs::hierarchy::ChildOf;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;

use crate::render::{NodeFields, build_child_plan, resolve_fields, apply_button, apply_card,
    apply_checkbox, apply_choice_picker, apply_column, apply_date_time_input, apply_divider,
    apply_flex_column, apply_icon, apply_media_placeholder, apply_modal, apply_row, apply_slider,
    apply_tabs, apply_text, apply_text_field};
use crate::state::{A2uiNode, A2uiState};

/// Which top-level root a planned node hangs under.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Root {
    /// The main surface pane.
    Surface,
    /// A Modal content overlay.
    Overlay,
}

/// One node in the planned tree.
struct PlanNode {
    id: String,
    kind: String,
    parent: Option<String>,
    root: Root,
    fields: NodeFields,
    focused: bool,
}

/// Plan the entire tree: the surface `root` + every open-Modal's `content`.
/// Returns the plan + the set of ids it touched (for orphan cleanup).
fn plan_tree(state: &A2uiState) -> (Vec<PlanNode>, std::collections::HashSet<String>) {
    let mut nodes = Vec::new();
    let mut touched = std::collections::HashSet::new();
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
        walk(
            &content_id,
            "",
            None,
            Root::Overlay,
            &components,
            &data_model,
            &state.functions,
            focused_id.as_deref(),
            &mut nodes,
            &mut touched,
        );
    }

    (nodes, touched)
}

/// Depth-first walk that emits a `PlanNode` for `id` then recurses into its
/// children (honoring the three A2UI child shapes via [`build_child_plan`]).
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
    nodes: &mut Vec<PlanNode>,
    touched: &mut std::collections::HashSet<String>,
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

    nodes.push(PlanNode {
        id: id.to_string(),
        kind,
        parent,
        root,
        fields,
        focused,
    });

    // Modal: walk the trigger in-tree (content is handled as an overlay above).
    if model.component_type == "Modal" {
        if let Some(trigger_id) = model.get_property::<String>("trigger") {
            walk(&trigger_id, base_path, Some(id.to_string()), root,
                components, data_model, functions, focused_id, nodes, touched);
        }
        return;
    }

    for (child_id, child_base) in build_child_plan(model, &ctx) {
        walk(&child_id, &child_base, Some(id.to_string()), root,
            components, data_model, functions, focused_id, nodes, touched);
    }
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

        // Apply kind + fields to the entity (idempotent).
        apply_kind(commands.entity(entity), node);

        // On first spawn of a TextField, seed its buffer with the resolved
        // value via the widget's own action queue (applied by its system).
        if is_new && node.kind == "TextField" && !node.fields.text.is_empty() {
            let text = node.fields.text.clone();
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
fn apply_kind(cmd: EntityCommands, node: &PlanNode) {
    match node.kind.as_str() {
        "Column" | "List" => apply_column(cmd),
        "Row" => apply_row(cmd),
        "Card" => apply_card(cmd),
        "Tabs" => apply_tabs(cmd),
        "Modal" => apply_modal(cmd),

        "Text" => apply_text(cmd, &node.fields),
        "Divider" => apply_divider(cmd),
        "Icon" => apply_icon(cmd, &node.fields),
        "DateTimeInput" => apply_date_time_input(cmd, &node.fields),
        "Image" => apply_media_placeholder(cmd, "Image", &node.fields),
        "Video" => apply_media_placeholder(cmd, "Video", &node.fields),
        "AudioPlayer" => apply_media_placeholder(cmd, "Audio", &node.fields),
        "ChoicePicker" => apply_choice_picker(cmd, &node.fields),

        "Button" => apply_button(cmd, &node.fields),
        "TextField" => apply_text_field(cmd, &node.fields, node.focused),
        "CheckBox" => apply_checkbox(cmd, &node.fields),
        "Slider" => apply_slider(cmd, &node.fields),

        _ => {
            // Unknown — a placeholder label + recurse (children are planned).
            apply_flex_column(cmd);
        }
    }
}
