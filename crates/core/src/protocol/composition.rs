//! Composition policies over schema-derived edges, independent of reachability policy.
use crate::catalog::definition::CatalogDefinition;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug)]
pub(crate) struct CompositionError {
    pub code: &'static str,
    pub component_id: String,
    pub message: String,
}
impl CompositionError {
    fn invalid(id: &str, message: impl Into<String>) -> Self {
        Self {
            code: "VALIDATION_FAILED",
            component_id: id.into(),
            message: message.into(),
        }
    }
}
#[derive(Default)]
struct Policies {
    parents: Vec<Vec<String>>,
    children: Vec<Vec<String>>,
}
fn policies(
    schema: &Value,
    root: &Value,
    id: &str,
    depth: usize,
    visits: &mut usize,
) -> Result<Policies, CompositionError> {
    if depth > 64 || *visits == 0 {
        return Err(CompositionError::invalid(
            id,
            "Composition schema traversal limit exceeded",
        ));
    }
    *visits -= 1;
    let mut result = Policies::default();
    for (key, target) in [
        ("allowedParents", &mut result.parents),
        ("allowedChildren", &mut result.children),
    ] {
        if let Some(value) = schema.get(key) {
            let entries = value.as_array().ok_or_else(|| {
                CompositionError::invalid(
                    id,
                    format!("{key} must be an array of component type names"),
                )
            })?;
            let list = entries
                .iter()
                .map(|entry| {
                    entry.as_str().map(str::to_owned).ok_or_else(|| {
                        CompositionError::invalid(id, format!("{key} entries must be strings"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            target.push(list);
        }
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let target = reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
            .ok_or_else(|| {
                CompositionError::invalid(id, "Unresolved composition schema reference")
            })?;
        let inherited = policies(target, root, id, depth + 1, visits)?;
        result.parents.extend(inherited.parents);
        result.children.extend(inherited.children);
    }
    if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
        for branch in branches {
            let inherited = policies(branch, root, id, depth + 1, visits)?;
            result.parents.extend(inherited.parents);
            result.children.extend(inherited.children);
        }
    }
    for key in ["anyOf", "oneOf", "if", "then", "else", "not"] {
        if let Some(branches) = schema.get(key) {
            let branches = branches
                .as_array()
                .map(|array| array.iter().collect::<Vec<_>>())
                .unwrap_or(vec![branches]);
            for branch in branches {
                let conditional = policies(branch, root, id, depth + 1, visits)?;
                if !conditional.parents.is_empty() || !conditional.children.is_empty() {
                    return Err(CompositionError::invalid(
                        id,
                        "Conditional composition restrictions are unsupported; declare them on the component type or allOf",
                    ));
                }
            }
        }
    }
    Ok(result)
}
struct Node {
    kind: String,
    policies: Policies,
}
fn list(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|v| format!("'{v}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn check_parent(
    child_id: &str,
    child: &Node,
    parent_id: &str,
    parent_kind: &str,
) -> Result<(), CompositionError> {
    for allowed in &child.policies.parents {
        if !allowed.iter().any(|name| name == parent_kind) {
            return Err(CompositionError {
                code: "UNALLOWED_PARENT",
                component_id: child_id.into(),
                message: format!(
                    "Component '{child_id}' ({}) cannot be placed under parent '{parent_id}' ({parent_kind}). Allowed parents: {}.",
                    child.kind,
                    list(allowed)
                ),
            });
        }
    }
    Ok(())
}
/// The caller supplies only actual schema-derived edges. An unreferenced node is
/// not implicitly a Surface child. Root is always checked against Surface even
/// if a separate component also references it. Every real parent is checked.
pub(crate) fn validate_composition(
    components: &BTreeMap<String, Value>,
    edges: &BTreeMap<String, Vec<String>>,
    catalogs: &BTreeMap<String, CatalogDefinition>,
    default_catalog: &str,
    root: &str,
) -> Result<(), CompositionError> {
    let mut nodes = BTreeMap::new();
    let mut visits = 100_000;
    for (id, component) in components {
        let catalog = catalogs
            .get(component["@catalogId"].as_str().unwrap_or(default_catalog))
            .ok_or_else(|| CompositionError::invalid(id, "Unknown component catalog"))?;
        let kind = component["component"]
            .as_str()
            .ok_or_else(|| CompositionError::invalid(id, "Missing component type"))?;
        let schema = catalog.resolved_schema()["components"]
            .get(kind)
            .ok_or_else(|| CompositionError::invalid(id, "Unknown component type"))?;
        nodes.insert(
            id,
            Node {
                kind: kind.into(),
                policies: policies(schema, catalog.resolved_schema(), id, 0, &mut visits)?,
            },
        );
    }
    if let Some(node) = nodes.get(&root.to_owned()) {
        check_parent(root, node, "Surface", "Surface")?;
    }
    for (parent_id, children) in edges {
        let parent = nodes
            .get(parent_id)
            .ok_or_else(|| CompositionError::invalid(parent_id, "Unknown parent component"))?;
        for child_id in children {
            let Some(child) = nodes.get(child_id) else {
                continue;
            }; // Dangling-reference policy belongs to topology validation.
            check_parent(child_id, child, parent_id, &parent.kind)?;
            for allowed in &parent.policies.children {
                if !allowed.iter().any(|name| name == &child.kind) {
                    return Err(CompositionError {
                        code: "UNALLOWED_CHILD",
                        component_id: parent_id.clone(),
                        message: format!(
                            "Container '{parent_id}' ({}) cannot contain child '{child_id}' ({}). Allowed children: {}.",
                            parent.kind,
                            child.kind,
                            list(allowed)
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn catalogs(schema: Value) -> BTreeMap<String, CatalogDefinition> {
        BTreeMap::from([(
            "c".into(),
            CatalogDefinition::from_json(
                json!({"catalogId":"c","protocolVersion":"1.0","components":schema}),
                None,
                None,
            )
            .unwrap(),
        )])
    }
    fn components(entries: &[(&str, &str)]) -> BTreeMap<String, Value> {
        entries
            .iter()
            .map(|(id, kind)| (id.to_string(), json!({"id":id,"component":kind})))
            .collect()
    }
    fn edges(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(id, children)| {
                (
                    id.to_string(),
                    children.iter().map(|v| v.to_string()).collect(),
                )
            })
            .collect()
    }
    #[test]
    fn root_is_surface_child_even_when_another_component_references_it() {
        let catalogs = catalogs(json!({"Card":{"allowedParents":["Box"]},"Box":{}}));
        let nodes = components(&[("root", "Card"), ("box", "Box")]);
        let error = validate_composition(
            &nodes,
            &edges(&[("box", &["root"])]),
            &catalogs,
            "c",
            "root",
        )
        .unwrap_err();
        assert_eq!(error.code, "UNALLOWED_PARENT");
        assert!(error.message.contains("'Surface' (Surface)"));
    }
    #[test]
    fn every_parent_is_checked_regardless_of_parent_order() {
        let catalogs = catalogs(json!({"Box":{},"Card":{},"Header":{"allowedParents":["Box"]}}));
        for (bad, good) in [("a", "z"), ("z", "a")] {
            let nodes = components(&[
                ("root", "Box"),
                (bad, "Card"),
                (good, "Box"),
                ("h", "Header"),
            ]);
            let error = validate_composition(
                &nodes,
                &edges(&[("root", &[bad, good]), (bad, &["h"]), (good, &["h"])]),
                &catalogs,
                "c",
                "root",
            )
            .unwrap_err();
            assert_eq!(error.component_id, "h");
            assert_eq!(error.code, "UNALLOWED_PARENT");
        }
    }
    #[test]
    fn dropped_child_is_not_implicitly_reparented_to_surface() {
        let catalogs = catalogs(
            json!({"Box":{"allowedParents":["Surface"]},"Header":{"allowedParents":["Box"]}}),
        );
        let nodes = components(&[("root", "Box"), ("h", "Header")]);
        validate_composition(&nodes, &edges(&[("root", &[])]), &catalogs, "c", "root").unwrap();
    }
    #[test]
    fn containers_restrict_children_and_empty_list_allows_none() {
        for allowed in [json!(["Text", "Button"]), json!([])] {
            let catalogs = catalogs(json!({"Box":{"allowedChildren":allowed},"Video":{}}));
            let error = validate_composition(
                &components(&[("root", "Box"), ("v", "Video")]),
                &edges(&[("root", &["v"])]),
                &catalogs,
                "c",
                "root",
            )
            .unwrap_err();
            assert_eq!(error.code, "UNALLOWED_CHILD");
            assert_eq!(error.component_id, "root");
        }
    }
    #[test]
    fn policies_follow_each_components_actual_catalog() {
        let mut catalogs = catalogs(json!({"Box":{},"Header":{}}));
        catalogs.insert("other".into(),CatalogDefinition::from_json(json!({"catalogId":"other","protocolVersion":"1.0","components":{"Header":{"allowedParents":["Card"]}}}),None,None).unwrap());
        let mut nodes = components(&[("root", "Box"), ("h", "Header")]);
        nodes.get_mut("h").unwrap()["@catalogId"] = json!("other");
        assert_eq!(
            validate_composition(&nodes, &edges(&[("root", &["h"])]), &catalogs, "c", "root")
                .unwrap_err()
                .code,
            "UNALLOWED_PARENT"
        );
    }
    #[test]
    fn allof_constraints_are_conjunctive_and_conditional_constraints_fail_closed() {
        let root = json!({"$defs":{"base":{"allowedParents":["Surface"]}}});
        let rules = policies(
            &json!({"$ref":"#/$defs/base","allOf":[{"allowedParents":["Box"]}]}),
            &root,
            "root",
            0,
            &mut 100,
        )
        .unwrap();
        assert_eq!(rules.parents.len(), 2);
        assert!(
            policies(
                &json!({"anyOf":[{"allowedParents":["Surface"]},{}]}),
                &root,
                "root",
                0,
                &mut 100
            )
            .is_err()
        );
        assert!(
            policies(
                &json!({"allowedChildren":"Text"}),
                &root,
                "root",
                0,
                &mut 100
            )
            .is_err()
        );
    }
}
