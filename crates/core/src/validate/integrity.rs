//! Component integrity + recursion/path validation.
//!
//! Ports `integrity_checker.py`. Operates on raw `&serde_json::Value` (not
//! `ComponentModel`) so it can run on incoming message payloads before they are
//! parsed into the internal model.
//!
//! v0.9-flat ONLY. Python's v0.8 structured-component shape
//! (`{"component": {"Type": {...}}}`) and its arbitrary-nested `componentId`
//! recursion (`extract_pointers`) are intentionally omitted: `ComponentModel`
//! already rejects the v0.8 shape, so the validator stays consistent with the
//! existing parser. In v0.9-flat, a `componentId` only ever appears inside a
//! `children` `Template` object.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::error::{ValidationError, ValidationReport};
use super::ref_fields::RefFieldSpec;

/// Maximum nesting depth of the whole JSON tree (mirrors Python
/// `MAX_GLOBAL_DEPTH`). Guards against pathological payloads.
pub const MAX_GLOBAL_DEPTH: u32 = 50;
/// Maximum nesting depth of function-call (`{call, args}`) chains (mirrors
/// Python `MAX_FUNC_CALL_DEPTH`).
pub const MAX_FUNC_CALL_DEPTH: u32 = 5;

/// Relaxed JSON-Pointer-ish path syntax, ported verbatim from Python
/// `RELAXED_PATH_PATTERN`. Allows `/seg/seg` and bare `seg` forms, where each
/// segment char is `[^~/]` or an escape `~0`/`~1`.
///
/// Uses `LazyLock` (stable since Rust 1.80). If the toolchain is older, swap
/// for `OnceLock` + manual `get_or_init`.
static RELAXED_PATH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(?:/(?:[^~/]|~[01])*)*|(?:[^~/]|~[01])+(?:/(?:[^~/]|~[01])*)*)$")
        .expect("RELAXED_PATH_PATTERN is a compile-time-constant regex")
});

/// Extract every `(ref_id, field_name)` pair from a single v0.9-flat
/// component object.
///
/// For `single_refs` keys: if the value is a string, yield `(string, key)`.
/// For `list_refs` keys:
/// - if the value is an array, yield each string item as `(item, "key[i]")`;
/// - if the value is an object containing `componentId` (the v0.9 `Template`
///   shape), yield `(obj["componentId"], "key.componentId")`.
///
/// Non-matching shapes are silently ignored (graceful).
pub fn get_component_references(component: &Value, spec: &RefFieldSpec) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    let Some(obj) = component.as_object() else {
        return refs;
    };

    // single refs: child, activeTab, ...
    for &key in spec.single_refs {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            refs.push((s.to_string(), key.to_string()));
        }
    }

    // list refs: children (Static array | Template {componentId, path})
    for &key in spec.list_refs {
        let Some(val) = obj.get(key) else {
            continue;
        };
        match val {
            Value::Array(arr) => {
                for (i, item) in arr.iter().enumerate() {
                    if let Some(s) = item.as_str() {
                        refs.push((s.to_string(), format!("{key}[{i}]")));
                    }
                }
            }
            Value::Object(o) => {
                // Template shape: { "componentId": "<id>", "path": "..." }
                if let Some(cid) = o.get("componentId").and_then(|v| v.as_str()) {
                    refs.push((cid.to_string(), format!("{key}.componentId")));
                }
            }
            _ => {}
        }
    }

    refs
}

/// Validate component integrity: duplicate ids, missing root, dangling
/// references. Collects ALL errors (does not short-circuit), returning a
/// `ValidationReport`.
///
/// `allow_dangling_references` skips the dangling check entirely (incremental
/// update refs may live elsewhere). `allow_missing_root` skips the missing-root
/// check.
pub fn validate_component_integrity(
    components: &[Value],
    spec: &RefFieldSpec,
    root_id: &str,
    allow_dangling_references: bool,
    allow_missing_root: bool,
) -> ValidationReport {
    let mut report = ValidationReport::new();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // 1. Collect IDs, flag duplicates.
    for comp in components {
        let Some(comp_id) = comp
            .as_object()
            .and_then(|o| o.get("id"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        if !ids.insert(comp_id.to_string()) {
            report.push(ValidationError::duplicate_id(comp_id));
        }
    }

    // Incremental update: referenced ids may already be on the client — skip
    // both the root and dangling checks.
    if allow_dangling_references {
        return report;
    }

    // 2. Missing root.
    if !allow_missing_root && !ids.contains(root_id) {
        report.push(ValidationError::missing_root(root_id));
    }

    // 3. Dangling references.
    for comp in components {
        let comp_id = comp
            .as_object()
            .and_then(|o| o.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        for (ref_id, field) in get_component_references(comp, spec) {
            if !ids.contains(&ref_id) {
                report.push(ValidationError::dangling(comp_id, &ref_id, &field));
            }
        }
    }

    report
}

/// Validate recursion depth and `path` syntax across an arbitrary JSON value.
///
/// - Global nesting depth > `MAX_GLOBAL_DEPTH` → `GlobalDepthExceeded`.
/// - A `path` string that does not match `RELAXED_PATH_PATTERN` →
///   `InvalidPathSyntax`.
/// - A v0.9 function-call object (`{call, args}`) nested deeper than
///   `MAX_FUNC_CALL_DEPTH` → `FuncCallDepthExceeded`.
///
/// Collects all errors found (does not stop at the first).
pub fn validate_recursion_and_paths(data: &Value) -> ValidationReport {
    let mut report = ValidationReport::new();
    traverse(data, 0, 0, &mut report);
    report
}

fn traverse(item: &Value, global_depth: u32, func_depth: u32, report: &mut ValidationReport) {
    if global_depth > MAX_GLOBAL_DEPTH {
        report.push(ValidationError::global_depth("<anon>"));
        // Don't recurse further — we've already flagged this branch.
        return;
    }

    match item {
        Value::Array(arr) => {
            for x in arr {
                traverse(x, global_depth + 1, func_depth, report);
            }
        }
        Value::Object(obj) => {
            // path syntax check
            if let Some(p) = obj.get("path").and_then(|v| v.as_str())
                && !RELAXED_PATH_PATTERN.is_match(p)
            {
                report.push(ValidationError::invalid_path(p));
            }

            // v0.9 function-call shape: has both "call" and "args".
            // (Python also handles a v0.8 "functionCall" object; v0.9-flat
            // payloads don't use it, so it's omitted here.)
            let is_func_v09 = obj.get("call").is_some() && obj.get("args").is_some();
            if is_func_v09 {
                if func_depth >= MAX_FUNC_CALL_DEPTH {
                    report.push(ValidationError::func_depth());
                    // Still recurse into args so deeper violations are caught,
                    // but the limit has already been flagged for this node.
                    for (k, v) in obj {
                        if k == "args" {
                            traverse(v, global_depth + 1, func_depth + 1, report);
                        } else {
                            traverse(v, global_depth + 1, func_depth, report);
                        }
                    }
                    return;
                }
                for (k, v) in obj {
                    if k == "args" {
                        traverse(v, global_depth + 1, func_depth + 1, report);
                    } else {
                        traverse(v, global_depth + 1, func_depth, report);
                    }
                }
            } else {
                for v in obj.values() {
                    traverse(v, global_depth + 1, func_depth, report);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::error::ValidationErrorCode;
    use crate::validate::ref_fields::RefFieldSpec;
    use serde_json::json;

    // Note: these tests use the v0.9-flat shape (id + component string at top
    // level), matching how Rust payloads actually arrive.

    fn spec() -> RefFieldSpec {
        RefFieldSpec::DEFAULT
    }

    // -- get_component_references --

    #[test]
    fn refs_extract_child_string() {
        let comp = json!({ "id": "c1", "component": "Box", "child": "child1" });
        let refs = get_component_references(&comp, &spec());
        assert!(refs.iter().any(|(r, _)| r == "child1"));
    }

    #[test]
    fn refs_extract_children_array() {
        let comp = json!({ "id": "c1", "component": "Column", "children": ["a", "b"] });
        let refs = get_component_references(&comp, &spec());
        let ids: Vec<&str> = refs.iter().map(|(r, _)| r.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
    }

    #[test]
    fn refs_extract_children_template_component_id() {
        let comp = json!({
            "id": "c1", "component": "Column",
            "children": { "componentId": "card", "path": "/items" }
        });
        let refs = get_component_references(&comp, &spec());
        assert!(
            refs.iter()
                .any(|(r, f)| r == "card" && f == "children.componentId")
        );
    }

    #[test]
    fn refs_extract_active_tab() {
        let comp = json!({ "id": "c1", "component": "Tabs", "activeTab": "tab1" });
        let refs = get_component_references(&comp, &spec());
        assert!(refs.iter().any(|(r, _)| r == "tab1"));
    }

    // -- validate_component_integrity --

    #[test]
    fn integrity_valid_no_errors() {
        let components = vec![
            json!({ "id": "root", "component": "Column", "children": ["c1"] }),
            json!({ "id": "c1", "component": "Text", "text": "hi" }),
        ];
        let r = validate_component_integrity(&components, &spec(), "root", false, false);
        assert!(r.is_empty(), "expected no errors, got: {r}");
    }

    #[test]
    fn integrity_duplicate_id() {
        let components = vec![
            json!({ "id": "c1", "component": "Box" }),
            json!({ "id": "c1", "component": "Text" }),
        ];
        let r = validate_component_integrity(&components, &spec(), "root", false, true);
        assert!(r.has_code(&ValidationErrorCode::DuplicateId));
    }

    #[test]
    fn integrity_missing_root() {
        let components = vec![json!({ "id": "c1", "component": "Box" })];
        let r = validate_component_integrity(&components, &spec(), "root", false, false);
        assert!(r.has_code(&ValidationErrorCode::MissingRoot));
    }

    #[test]
    fn integrity_dangling_ref() {
        let components = vec![json!({ "id": "root", "component": "Box", "child": "nonexistent" })];
        let r = validate_component_integrity(&components, &spec(), "root", false, false);
        assert!(r.has_code(&ValidationErrorCode::DanglingReference));
    }

    // -- validate_recursion_and_paths --

    #[test]
    fn recursion_valid_path() {
        let data = json!({ "path": "/valid/path", "nested": [{ "path": "/another" }] });
        let r = validate_recursion_and_paths(&data);
        assert!(r.is_empty(), "expected no errors, got: {r}");
    }

    #[test]
    fn recursion_invalid_path_syntax() {
        let data = json!({ "path": "invalid~path//double" });
        let r = validate_recursion_and_paths(&data);
        assert!(r.has_code(&ValidationErrorCode::InvalidPathSyntax));
    }

    #[test]
    fn recursion_global_depth_exceeded() {
        // 52-deep nested array.
        let mut deep = json!(null);
        for _ in 0..52 {
            deep = json!([deep]);
        }
        let r = validate_recursion_and_paths(&deep);
        assert!(r.has_code(&ValidationErrorCode::GlobalDepthExceeded));
    }

    #[test]
    fn recursion_func_call_depth_exceeded() {
        // 6-deep {call, args} chain.
        let mut deep = json!({});
        for _ in 0..6 {
            deep = json!({ "call": "func", "args": deep });
        }
        let r = validate_recursion_and_paths(&deep);
        assert!(r.has_code(&ValidationErrorCode::FuncCallDepthExceeded));
    }

    #[test]
    fn relaxed_allows_dangling_and_missing_root() {
        let components = vec![json!({ "id": "root", "component": "Box", "child": "ghost" })];
        // allow_dangling_references=true short-circuits before the root/dangling
        // checks, so neither MissingRoot nor DanglingReference is reported.
        let r = validate_component_integrity(&components, &spec(), "root", true, true);
        assert!(r.is_empty(), "expected no errors under RELAXED, got: {r}");
    }
}
