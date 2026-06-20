//! Topology analysis — DFS cycle / self-reference / orphan / depth detection.
//!
//! Ports `topology_analyzer.py`.
//!
//! DELIBERATE IMPROVEMENT over the Python original: Python raises on the first
//! cycle/orphan and relies on the outer `validate_components` try/except to
//! collect. The Rust `analyze_topology` instead collects ALL cycles, orphans,
//! and depth violations found during a full traversal into the returned
//! `ValidationReport`, rather than stopping at the first. This gives callers a
//! complete diagnostic picture in one pass.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::error::{ValidationError, ValidationReport};
use super::integrity::{MAX_GLOBAL_DEPTH, get_component_references};
use super::ref_fields::RefFieldSpec;

/// Analyze the component reference graph.
///
/// Builds an adjacency list `id -> [ref_id]` from `get_component_references`,
/// then DFS-traverses it to detect:
/// - self-reference (a node referencing itself → `SelfReference`),
/// - cycles (a back-edge to a node on the current DFS stack →
///   `CircularReference`),
/// - excessive depth (> `MAX_GLOBAL_DEPTH` → `GlobalDepthExceeded`).
///
/// After traversal, if `!allow_orphan_components`, any id never visited from
/// the root → `OrphanComponent`.
///
/// Returns `(visited_set, report)`.
pub fn analyze_topology(
    components: &[Value],
    spec: &RefFieldSpec,
    root_id: &str,
    allow_orphan_components: bool,
    allow_missing_root: bool,
) -> (HashSet<String>, ValidationReport) {
    let mut report = ValidationReport::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_ids: HashSet<String> = HashSet::new();

    // Build adjacency list.
    for comp in components {
        let Some(comp_id) = comp
            .as_object()
            .and_then(|o| o.get("id"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        all_ids.insert(comp_id.to_string());
        adj.entry(comp_id.to_string()).or_default();

        for (ref_id, field) in get_component_references(comp, spec) {
            if ref_id == comp_id {
                report.push(ValidationError::self_ref(comp_id, &field));
                continue;
            }
            adj.entry(comp_id.to_string()).or_default().push(ref_id);
        }
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut on_stack: HashSet<String> = HashSet::new();

    if allow_missing_root {
        // No single root: traverse from every node to catch all cycles.
        // Sort for deterministic error ordering.
        let mut start_nodes: Vec<String> = all_ids.iter().cloned().collect();
        start_nodes.sort();
        for node_id in start_nodes {
            if !visited.contains(&node_id) {
                dfs(&node_id, &adj, &mut visited, &mut on_stack, 0, &mut report);
            }
        }
    } else {
        if all_ids.contains(root_id) {
            dfs(root_id, &adj, &mut visited, &mut on_stack, 0, &mut report);
        }
        // Orphans: ids present but never visited from root.
        if !allow_orphan_components {
            let mut orphans: Vec<String> = all_ids
                .iter()
                .filter(|id| !visited.contains(*id))
                .cloned()
                .collect();
            orphans.sort();
            for orphan in orphans {
                report.push(ValidationError::orphan(&orphan, root_id));
            }
        }
    }

    (visited, report)
}

/// Explicit recursive DFS (not a closure) so the borrow checker is happy with
/// multiple `&mut` references. Recursion is bounded by `MAX_GLOBAL_DEPTH`, so
/// stack overflow is not a concern.
fn dfs(
    node_id: &str,
    adj: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    on_stack: &mut HashSet<String>,
    depth: u32,
    report: &mut ValidationReport,
) {
    if depth > MAX_GLOBAL_DEPTH {
        report.push(ValidationError::global_depth(node_id));
        return;
    }

    visited.insert(node_id.to_string());
    on_stack.insert(node_id.to_string());

    if let Some(neighbors) = adj.get(node_id) {
        for nb in neighbors {
            if !visited.contains(nb) {
                dfs(nb, adj, visited, on_stack, depth + 1, report);
            } else if on_stack.contains(nb) {
                report.push(ValidationError::circular(nb));
            }
        }
    }

    on_stack.remove(node_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::error::ValidationErrorCode;
    use crate::validate::ref_fields::RefFieldSpec;
    use serde_json::json;

    fn spec() -> RefFieldSpec {
        // The tests below use v0.9-flat shape with "child" as the single ref,
        // which DEFAULT already covers.
        RefFieldSpec::DEFAULT
    }

    #[test]
    fn valid_chain_visits_root_and_child() {
        let components = vec![
            json!({ "id": "root", "component": "Node", "child": "n1" }),
            json!({ "id": "n1", "component": "Node" }),
        ];
        let (visited, report) = analyze_topology(&components, &spec(), "root", false, false);
        assert!(report.is_empty(), "unexpected errors: {report}");
        let expected: HashSet<String> = ["root", "n1"].into_iter().map(String::from).collect();
        assert_eq!(visited, expected);
    }

    #[test]
    fn self_reference_detected() {
        let components = vec![json!({ "id": "root", "component": "Node", "child": "root" })];
        let (_visited, report) = analyze_topology(&components, &spec(), "root", false, false);
        assert!(report.has_code(&ValidationErrorCode::SelfReference));
    }

    #[test]
    fn circular_reference_detected() {
        let components = vec![
            json!({ "id": "root", "component": "Node", "child": "n1" }),
            json!({ "id": "n1", "component": "Node", "child": "root" }),
        ];
        let (_visited, report) = analyze_topology(&components, &spec(), "root", false, false);
        assert!(report.has_code(&ValidationErrorCode::CircularReference));
    }

    #[test]
    fn orphan_strict_reports_error() {
        let components = vec![
            json!({ "id": "root", "component": "Node" }),
            json!({ "id": "orphan", "component": "Node" }),
        ];
        let (_visited, report) = analyze_topology(&components, &spec(), "root", false, false);
        assert!(report.has_code(&ValidationErrorCode::OrphanComponent));
    }

    #[test]
    fn orphan_relaxed_allowed() {
        let components = vec![
            json!({ "id": "root", "component": "Node" }),
            json!({ "id": "orphan", "component": "Node" }),
        ];
        let (_visited, report) = analyze_topology(&components, &spec(), "root", true, false);
        assert!(
            report.is_empty(),
            "orphan should be allowed under RELAXED, got: {report}"
        );
    }

    #[test]
    fn coverage_combined_orphan_cycle_self() {
        // All three failure modes in one payload (mirrors test_validating.py
        // `test_topology_cyclomatic_orphans_coverage`).
        //
        // 1. orphan
        let orphan_components = vec![
            json!({ "id": "root", "component": "Node", "child": "A" }),
            json!({ "id": "A", "component": "Node" }),
            json!({ "id": "B", "component": "Node" }),
        ];
        let (_, r) = analyze_topology(&orphan_components, &spec(), "root", false, false);
        assert!(r.has_code(&ValidationErrorCode::OrphanComponent));

        // 2. cycle: root -> A -> B -> A
        let cycle_components = vec![
            json!({ "id": "root", "component": "Node", "child": "A" }),
            json!({ "id": "A", "component": "Node", "child": "B" }),
            json!({ "id": "B", "component": "Node", "child": "A" }),
        ];
        let (_, r) = analyze_topology(&cycle_components, &spec(), "root", true, false);
        assert!(r.has_code(&ValidationErrorCode::CircularReference));

        // 3. self-reference
        let self_components = vec![json!({ "id": "root", "component": "Node", "child": "root" })];
        let (_, r) = analyze_topology(&self_components, &spec(), "root", true, false);
        assert!(r.has_code(&ValidationErrorCode::SelfReference));
    }
}
