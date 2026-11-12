use a2ui_base::{
    catalog::definition::CatalogDefinition, protocol::strict_current::StrictCurrentValidator,
};
use serde_json::{Value, json};
fn basic() -> Value {
    serde_json::from_str(include_str!("conformance/catalogs/basic_v1.json")).unwrap()
}
#[test]
fn official_topology_audit() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/validator_v1_0.yaml")).unwrap();
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/strict-validator-coverage.json")).unwrap();
    assert_eq!(cases.len(), 49);
    let included = manifest["included"].as_array().unwrap();
    let excluded = manifest["excluded"].as_array().unwrap();
    assert_eq!(included.len() + excluded.len(), cases.len());
    assert!(manifest["known_divergences"].as_array().unwrap().is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        if !included.iter().any(|n| n == name) {
            assert!(excluded.iter().any(|n| n["name"] == name));
            continue;
        }
        let mut validator = StrictCurrentValidator::new();
        let raw = case.get("catalog").cloned().unwrap_or_else(basic);
        let definition = CatalogDefinition::from_json(raw, None, Some("1.0")).unwrap();
        validator.register_catalog(definition, true).unwrap();
        // Official fixture uses both historical and current basic catalog aliases.
        validator
            .register_catalog(
                CatalogDefinition::from_json(
                    basic(),
                    Some("https://a2ui.org/catalogs/basic/v1/catalog.json"),
                    Some("1.0"),
                )
                .unwrap(),
                false,
            )
            .unwrap();
        let name = case["name"].as_str().unwrap();
        let mut problem = None;
        for step in case["steps"].as_array().unwrap() {
            let result = step["messages"]
                .as_array()
                .unwrap()
                .iter()
                .try_for_each(|m| validator.validate_message(m));
            match (result, step.get("expectError")) {
                (Ok(()), None) => {}
                (Err(error), Some(expected)) => {
                    if let Some(fields) = expected.as_object() {
                        assert!(
                            fields
                                .keys()
                                .all(|key| matches!(key.as_str(), "category" | "message")),
                            "unhandled expectation in {name}"
                        );
                    }
                    let message = expected
                        .as_str()
                        .or_else(|| expected["message"].as_str())
                        .unwrap_or("");
                    if !error.message.contains(message) {
                        problem = Some(format!("expected {message}: {error:?}"));
                    }
                    if let Some(category) = expected["category"].as_str() {
                        if !category_matches(category, &error.category) {
                            problem = Some(format!("expected {category}: {error:?}"));
                        }
                    }
                }
                (actual, expected) => {
                    problem = Some(format!("expected {expected:?} actual {actual:?}"))
                }
            }
        }
        if let Some(problem) = problem {
            failed.push(format!("{name}: {problem}"));
        } else {
            passed.push(name.to_string());
        }
    }
    eprintln!(
        "PASSED {} {passed:#?}\nFAILED {} {failed:#?}",
        passed.len(),
        failed.len()
    );
    assert!(failed.is_empty());
    assert_eq!(passed.len(), 33);
}
#[test]
fn invalid_update_is_atomic() {
    let mut v = StrictCurrentValidator::new();
    v.register_catalog(
        CatalogDefinition::from_json(basic(), None, Some("1.0")).unwrap(),
        true,
    )
    .unwrap();
    v.validate_message(&json!({"version":"1.0","createSurface":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["leaf"]},{"id":"leaf","component":"Text","text":"Hi"}]}})).unwrap();
    assert!(v.validate_message(&json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["missing"]}]}})).is_err());
    v.validate_message(&json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"leaf","component":"Text","text":"Still valid"}]}})).unwrap();
}

#[test]
fn mixed_catalog_components_and_functions_use_distinct_defaults() {
    let mut v = StrictCurrentValidator::new();
    v.register_catalog(
        CatalogDefinition::from_json(basic(), None, Some("1.0")).unwrap(),
        true,
    )
    .unwrap();
    let custom = json!({"catalogId":"custom","components":{"Label":{"type":"object","properties":{"component":{"const":"Label"},"label":{"$ref":"common_types.json#/$defs/DynamicString"}},"required":["component","label"]}},"functions":{"moon":{"type":"object","returnType":"string","allowedCallers":"rendererOnly","properties":{"@call":{"const":"moon"},"args":{"type":"object","properties":{"value":{"$ref":"common_types.json#/$defs/DynamicString"}},"required":["value"],"additionalProperties":false}},"required":["@call","args"]}}});
    v.register_catalog(
        CatalogDefinition::from_json(custom, None, Some("1.0")).unwrap(),
        false,
    )
    .unwrap();
    v.validate_message(&json!({"version":"1.0","createSurface":{"surfaceId":"s","components":[{"id":"root","component":"Label","@catalogId":"custom","label":{"@call":"formatString","args":{"value":"surface default"}}}]}})).unwrap();
    let mut update = json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Label","@catalogId":"custom","label":{"@call":"moon","catalogId":"custom","args":{"value":{"@call":"formatString","args":{"value":"nested surface default"}}}}}]}});
    v.validate_message(&update).unwrap();
    update["updateComponents"]["components"][0]["label"]["args"]["value"] = json!(123);
    assert!(v.validate_message(&update).is_err());
    update["updateComponents"]["components"][0]["label"] =
        json!({"@call":"moon","args":{"value":"no explicit function override"}});
    assert!(v.validate_message(&update).is_err());
    update["updateComponents"]["components"][0]["label"] =
        json!({"@call":"moon","catalogId":"unknown","args":{"value":"bad target"}});
    assert!(v.validate_message(&update).is_err());
    assert!(
        v.register_catalog(
            CatalogDefinition::from_json(basic(), None, Some("1.0")).unwrap(),
            true
        )
        .is_err()
    );
}

// Match pinned Python CATEGORY_TO_EXCEPTION + subclass checks, not string equality.
fn category_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        || (expected == "ValidationError" && matches!(actual, "IntegrityError" | "RecursionError"))
        || (expected == "IntegrityError" && actual == "RecursionError")
}
