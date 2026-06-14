//! Keyboard focus management — framework-agnostic.
//!
//! [`FocusManager`] maintains an ordered list of focusable component IDs
//! (collected via depth-first traversal of the component tree) and provides
//! Tab / Shift-Tab cycling. It depends only on core types, so every backend
//! (ratatui, Slint, …) shares a single implementation.
//!
//! Backends with native focus (e.g. Slint) may still use this to reproduce the
//! ratatui Tab order in tests, or to drive focus when native focus is disabled.

use crate::model::component_model::ComponentModel;
use crate::model::components_model::SurfaceComponentsModel;
use crate::protocol::common_types::ChildList;

/// Component types that can receive keyboard focus.
pub const FOCUSABLE_TYPES: &[&str] = &[
    "Button",
    "TextField",
    "CheckBox",
    "Slider",
    "ChoicePicker",
    "DateTimeInput",
    // Interactive only under the tui `audio` feature, but listing it always is
    // harmless: without the feature its handle_event is the trait default
    // (no-op), so focusing it simply does nothing.
    "AudioPlayer",
];

/// Manages keyboard focus across interactive components.
pub struct FocusManager {
    /// Ordered list of focusable component IDs (depth-first traversal order).
    pub focusable_ids: Vec<String>,
    /// Current focus index into `focusable_ids`.
    current_index: usize,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    /// Create an empty focus manager.
    pub fn new() -> Self {
        Self {
            focusable_ids: Vec::new(),
            current_index: 0,
        }
    }

    /// Rebuild the focus list by traversing the component tree depth-first.
    ///
    /// Call this whenever the component tree changes (e.g. surface update).
    /// Components that are present in `focusable_ids` retain their focus if
    /// the focused ID still exists after the rebuild.
    pub fn rebuild_from_components(&mut self, components: &SurfaceComponentsModel) {
        let previously_focused = self.focused_id().map(|s| s.to_string());

        self.focusable_ids.clear();
        self.current_index = 0;

        // Find root components (those not referenced as any other component's child).
        let child_ids = collect_all_child_ids(components);
        let all = components.all();

        let mut roots: Vec<&ComponentModel> = all
            .values()
            .filter(|c| !child_ids.contains(c.id.as_str()))
            .collect();
        // Sort roots by ID for deterministic ordering.
        roots.sort_by(|a, b| a.id.cmp(&b.id));

        for root in &roots {
            self.collect_focusable_depth_first(root, components);
        }

        // Restore focus if the previously-focused ID still exists.
        if let Some(ref prev_id) = previously_focused {
            if let Some(idx) = self.focusable_ids.iter().position(|id| id == prev_id) {
                self.current_index = idx;
            }
        }
    }

    /// Move focus to the next focusable component (Tab).
    pub fn focus_next(&mut self) {
        if self.focusable_ids.is_empty() {
            return;
        }
        self.current_index = (self.current_index + 1) % self.focusable_ids.len();
    }

    /// Move focus to the previous focusable component (Shift+Tab).
    pub fn focus_prev(&mut self) {
        if self.focusable_ids.is_empty() {
            return;
        }
        if self.current_index == 0 {
            self.current_index = self.focusable_ids.len() - 1;
        } else {
            self.current_index -= 1;
        }
    }

    /// Returns `true` if the component with the given ID currently has focus.
    pub fn is_focused(&self, id: &str) -> bool {
        self.focusable_ids
            .get(self.current_index)
            .is_some_and(|focused| focused == id)
    }

    /// Returns the ID of the currently focused component, if any.
    pub fn focused_id(&self) -> Option<&str> {
        self.focusable_ids.get(self.current_index).map(|s| s.as_str())
    }

    /// Clear all focus state.
    pub fn reset(&mut self) {
        self.focusable_ids.clear();
        self.current_index = 0;
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Recursively collect focusable component IDs in depth-first order.
    fn collect_focusable_depth_first(
        &mut self,
        component: &ComponentModel,
        components: &SurfaceComponentsModel,
    ) {
        if FOCUSABLE_TYPES.contains(&component.component_type.as_str()) {
            self.focusable_ids.push(component.id.clone());
        }

        // Visit children.
        if let Some(child_ids) = component.children() {
            match child_ids {
                ChildList::Static(ids) => {
                    for cid in &ids {
                        if let Some(child) = components.get(cid) {
                            self.collect_focusable_depth_first(child, components);
                        }
                    }
                }
                ChildList::Template { .. } => {
                    // Template children are resolved at render time;
                    // focus management for dynamic children is not supported
                    // in this initial implementation.
                }
            }
        }

        // Single child (used by wrapper components like ScrollView).
        if let Some(single_id) = component.child() {
            if let Some(child) = components.get(&single_id) {
                self.collect_focusable_depth_first(child, components);
            }
        }
    }
}

/// Collect every component ID that appears as a child of another component.
fn collect_all_child_ids(components: &SurfaceComponentsModel) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    for component in components.all().values() {
        if let Some(ChildList::Static(children)) = component.children() {
            for cid in children {
                ids.insert(cid.clone());
            }
        }
        if let Some(single_id) = component.child() {
            ids.insert(single_id);
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_component(id: &str, component_type: &str) -> ComponentModel {
        ComponentModel::from_json(&json!({
            "id": id,
            "component": component_type,
        }))
        .unwrap()
    }

    fn make_container(id: &str, component_type: &str, child_ids: &[&str]) -> ComponentModel {
        ComponentModel::from_json(&json!({
            "id": id,
            "component": component_type,
            "children": child_ids,
        }))
        .unwrap()
    }

    #[test]
    fn collects_focusable_in_dfs_order() {
        let mut surface = SurfaceComponentsModel::new();
        // Tree: Column -> [Button1, Row -> [TextField, Button2]]
        surface.upsert(make_container("col", "Column", &["btn1", "row"]));
        surface.upsert(make_component("btn1", "Button"));
        surface.upsert(make_container("row", "Row", &["tf1", "btn2"]));
        surface.upsert(make_component("tf1", "TextField"));
        surface.upsert(make_component("btn2", "Button"));

        let mut fm = FocusManager::new();
        fm.rebuild_from_components(&surface);

        assert_eq!(fm.focusable_ids, vec!["btn1", "tf1", "btn2"]);
    }

    #[test]
    fn focus_cycles_forward() {
        let mut fm = FocusManager::new();
        fm.focusable_ids = vec!["a".into(), "b".into(), "c".into()];

        assert_eq!(fm.focused_id(), Some("a"));
        fm.focus_next();
        assert_eq!(fm.focused_id(), Some("b"));
        fm.focus_next();
        assert_eq!(fm.focused_id(), Some("c"));
        fm.focus_next();
        assert_eq!(fm.focused_id(), Some("a")); // wraps
    }

    #[test]
    fn focus_cycles_backward() {
        let mut fm = FocusManager::new();
        fm.focusable_ids = vec!["a".into(), "b".into(), "c".into()];

        fm.focus_prev();
        assert_eq!(fm.focused_id(), Some("c")); // wraps
        fm.focus_prev();
        assert_eq!(fm.focused_id(), Some("b"));
    }

    #[test]
    fn is_focused_checks_current() {
        let mut fm = FocusManager::new();
        fm.focusable_ids = vec!["a".into(), "b".into()];

        assert!(fm.is_focused("a"));
        assert!(!fm.is_focused("b"));
        fm.focus_next();
        assert!(!fm.is_focused("a"));
        assert!(fm.is_focused("b"));
    }

    #[test]
    fn reset_clears_everything() {
        let mut fm = FocusManager::new();
        fm.focusable_ids = vec!["a".into()];
        fm.focus_next();
        fm.reset();
        assert!(fm.focused_id().is_none());
        assert!(fm.focusable_ids.is_empty());
    }

    #[test]
    fn empty_surface_yields_no_focus() {
        let surface = SurfaceComponentsModel::new();
        let mut fm = FocusManager::new();
        fm.rebuild_from_components(&surface);
        assert!(fm.focused_id().is_none());
        fm.focus_next(); // no panic
        assert!(fm.focused_id().is_none());
    }

    #[test]
    fn rebuild_preserves_focus_if_still_present() {
        let mut surface = SurfaceComponentsModel::new();
        surface.upsert(make_container("col", "Column", &["btn1", "btn2"]));
        surface.upsert(make_component("btn1", "Button"));
        surface.upsert(make_component("btn2", "Button"));

        let mut fm = FocusManager::new();
        fm.rebuild_from_components(&surface);
        fm.focus_next(); // focus btn2
        assert_eq!(fm.focused_id(), Some("btn2"));

        // Rebuild — btn2 should still be focused.
        fm.rebuild_from_components(&surface);
        assert_eq!(fm.focused_id(), Some("btn2"));
    }

    #[test]
    fn non_focusable_types_are_skipped() {
        let mut surface = SurfaceComponentsModel::new();
        surface.upsert(make_component("txt", "Text"));
        surface.upsert(make_component("btn", "Button"));

        let mut fm = FocusManager::new();
        fm.rebuild_from_components(&surface);
        assert_eq!(fm.focusable_ids, vec!["btn"]);
    }
}
