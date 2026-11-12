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
    let mut divergences = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        if !included.iter().any(|n| n == name) {
            assert!(excluded.iter().any(|n| n["name"] == name));
            continue;
        }
        let known = manifest["known_divergences"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name);
        let mut validator = StrictCurrentValidator::new();
        let raw = case.get("catalog").cloned().unwrap_or_else(basic);
        let definition = CatalogDefinition::from_json(raw, None, Some("1.0")).unwrap();
        validator.register_catalog(definition, true);
        // Official fixture uses both historical and current basic catalog aliases.
        validator.register_catalog(
            CatalogDefinition::from_json(
                basic(),
                Some("https://a2ui.org/catalogs/basic/v1/catalog.json"),
                Some("1.0"),
            )
            .unwrap(),
            false,
        );
        let name = case["name"].as_str().unwrap();
        let mut problem = None;
        for step in case["steps"].as_array().unwrap() {
            let result = step["messages"]
                .as_array()
                .unwrap()
                .iter()
                .try_for_each(|m| validator.validate_message(m));
            if let Some(known) = known {
                let error = result
                    .expect_err("known diagnostic divergence must still reject invalid input");
                assert_eq!(error.category, known["actual_category"].as_str().unwrap());
                assert!(
                    error
                        .message
                        .contains(known["actual_message"].as_str().unwrap())
                );
                divergences += 1;
                continue;
            }
            match (result, step.get("expectError")) {
                (Ok(()), None) => {}
                (Err(error), Some(expected)) => {
                    let message = expected
                        .as_str()
                        .or_else(|| expected["message"].as_str())
                        .unwrap_or("");
                    if !error.message.contains(message) {
                        problem = Some(format!("expected {message}: {error:?}"));
                    }
                    if let Some(category) = expected["category"].as_str() {
                        if category != error.category {
                            problem = Some(format!("expected {category}: {error:?}"));
                        }
                    }
                }
                (actual, expected) => {
                    problem = Some(format!("expected {expected:?} actual {actual:?}"))
                }
            }
        }
        if known.is_some() {
            continue;
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
    assert_eq!(passed.len(), 30);
    assert_eq!(divergences, 3);
}
#[test]
fn invalid_update_is_atomic() {
    let mut v = StrictCurrentValidator::new();
    v.register_catalog(
        CatalogDefinition::from_json(basic(), None, Some("1.0")).unwrap(),
        true,
    );
    v.validate_message(&json!({"version":"1.0","createSurface":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["leaf"]},{"id":"leaf","component":"Text","text":"Hi"}]}})).unwrap();
    assert!(v.validate_message(&json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["missing"]}]}})).is_err());
    v.validate_message(&json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"leaf","component":"Text","text":"Still valid"}]}})).unwrap();
}
