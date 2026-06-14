//! Flat A2UI component map → Slint reactive node array.
//!
//! [`build_nodes`] walks a [`SurfaceModel`]'s component tree (root first),
//! resolving every dynamic value to a concrete string/bool, and returns a
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

use std::collections::HashMap;

use a2ui_core::catalog::function_api::FunctionImplementation;
use a2ui_core::model::component_context::ComponentContext;
use a2ui_core::model::components_model::SurfaceComponentsModel;
use a2ui_core::model::data_model::DataModel;
use a2ui_core::model::surface_model::SurfaceModel;
use a2ui_core::protocol::common_types::{ChildList, DynamicString};

use crate::ui::LiveNode;

/// Build the flat node array for a surface's `"root"` component.
///
/// Returns an empty `Vec` when the surface has no `root`. Index 0 is always the
/// root (the `Surface` component renders `Node{MAX_DEPTH}` at index 0).
pub fn build_nodes(
    surface: &SurfaceModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
) -> Vec<LiveNode> {
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();
    if !components.contains("root") {
        return Vec::new();
    }
    let mut builder = FlatBuilder { nodes: Vec::new() };
    builder.add(
        "root",
        "",
        &data_model,
        &components,
        functions,
        &surface.id,
        focused_id,
    );
    builder.nodes
}

/// Accumulator that flattens the tree into a `Vec<LiveNode>` with index children.
struct FlatBuilder {
    nodes: Vec<LiveNode>,
}

impl FlatBuilder {
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
    ) -> Option<usize> {
        let model = components.get(id)?;
        let kind = model.component_type.clone();

        // Resolve this node's display fields while we hold the context borrow,
        // then plan its children — both as fully-owned data so the recursive
        // `add` calls below don't fight the borrow.
        let (text, label, variant, child_plan) = {
            let ctx = ComponentContext::new(
                id.to_string(),
                surface_id.to_string(),
                data_model,
                components,
                functions,
                base_path,
                focused_id.map(|s| s.to_string()),
            );
            let (text, label, variant) = resolve_fields(&kind, &ctx, model);
            let plan = build_child_plan(model, &ctx);
            (text, label, variant, plan)
        };

        // Reserve this node's slot first so it keeps the lowest index, then
        // recurse into children (they take later indices), then fill the slot.
        let idx = self.nodes.len();
        self.nodes.push(empty_node());
        let mut child_indices: Vec<i32> = Vec::new();
        for (child_id, child_base) in child_plan {
            if let Some(child_idx) =
                self.add(&child_id, &child_base, data_model, components, functions, surface_id, focused_id)
            {
                child_indices.push(child_idx as i32);
            }
        }

        self.nodes[idx] = LiveNode {
            id: id.into(),
            kind: kind.into(),
            text: text.into(),
            label: label.into(),
            variant: variant.into(),
            focused: focused_id == Some(id),
            children: to_int_model(child_indices),
        };
        Some(idx)
    }
}

/// Extract the (text, label, variant) display fields for a component type.
///
/// Unknown kinds fall through with empty text (the generated `.slint` shows the
/// kind name so an unimplemented component is still visible).
fn resolve_fields(
    kind: &str,
    ctx: &ComponentContext,
    model: &a2ui_core::model::component_model::ComponentModel,
) -> (String, String, String) {
    let variant: String = model.get_property::<String>("variant").unwrap_or_default();
    match kind {
        "Text" => {
            let text = model
                .get_property::<DynamicString>("text")
                .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
                .unwrap_or_default();
            (text, String::new(), variant)
        }
        "TextField" => {
            let label = model
                .get_property::<DynamicString>("label")
                .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
                .unwrap_or_default();
            let value = model
                .get_property::<DynamicString>("value")
                .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
                .unwrap_or_default();
            (value, label, variant)
        }
        _ => (String::new(), String::new(), variant),
    }
}

/// Plan a node's children as `(child_id, child_base_path)` pairs, honoring all
/// three A2UI child shapes (`child`, static `children`, template `children`).
fn build_child_plan(
    model: &a2ui_core::model::component_model::ComponentModel,
    ctx: &ComponentContext,
) -> Vec<(String, String)> {
    let mut plan = Vec::new();
    let base = ctx.data_context.base_path().to_string();

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

/// Wrap a `Vec<i32>` into the `ModelRc` shape a `[int]` property expects.
fn to_int_model(indices: Vec<i32>) -> slint::ModelRc<i32> {
    slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(indices)))
}

/// A placeholder node used to reserve a slot before its children are recursed.
fn empty_node() -> LiveNode {
    LiveNode {
        id: "".into(),
        kind: "".into(),
        text: "".into(),
        label: "".into(),
        variant: "".into(),
        focused: false,
        children: slint::ModelRc::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_core::catalog::Catalog;
    use a2ui_core::message_processor::MessageProcessor;
    use slint::Model;

    /// Build a surface from `components_json` (describing `root` + children),
    /// optionally seeding `/data`, and return its flat node array.
    fn build(
        components_json: serde_json::Value,
        data: Option<serde_json::Value>,
        focused_id: Option<&str>,
    ) -> Vec<LiveNode> {
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

        let surface = processor.model.get_surface("test").expect("surface exists");
        let functions = HashMap::new();
        build_nodes(surface, &functions, focused_id)
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
        assert!(build_nodes(surface, &HashMap::new(), None).is_empty());
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
}
