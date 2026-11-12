//! Pinned schema-boundary audit. Full PayloadValidator and generated-model
//! conformance are explicitly separate from schema-backed acceptance checks.
use a2ui_base::protocol::{current_schema::CurrentSchemaValidator, rpc::FunctionResponse};
use serde_json::Value;
use std::collections::HashSet;

fn validator() -> CurrentSchemaValidator {
    CurrentSchemaValidator::new(
        serde_json::from_str(include_str!("conformance/catalogs/basic_v1.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn official_common_type_boundary() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/common_types.yaml")).unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/schema-coverage.json")).unwrap();
    let excluded: HashSet<&str> = manifest["common_type_excluded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(excluded.len() + 11, cases.len());
    let validator = validator();
    let mut count = 0;
    for case in &cases {
        if case["protocolVersion"] != "1.0" || case["action"] != "validate_common_type" {
            assert!(excluded.contains(case["name"].as_str().unwrap()));
            continue;
        }
        count += 1;
        let name = case["name"].as_str().unwrap();
        let definition = case["definition"].as_str().unwrap();
        for step in case["steps"].as_array().unwrap() {
            let actual = validator.validate_common_type(definition, &step["value"]);
            if let Some(expected) = step.get("expectError") {
                assert_eq!(
                    expected,
                    &serde_json::json!({"category":"ValidationError"}),
                    "unexpected assertion in {name}"
                );
                assert!(
                    actual.is_err(),
                    "{name}: accepted invalid {}",
                    step["value"]
                );
            } else {
                assert_eq!(
                    actual.unwrap_or_else(|e| panic!("{name}: {e}")),
                    step["value"],
                    "{name}"
                );
            }
            // Also exercise the actual native serde model where implemented.
            if definition == "FunctionResponse" {
                let typed = serde_json::from_value::<FunctionResponse>(step["value"].clone());
                if step.get("expectError").is_some() {
                    assert!(typed.is_err(), "{name}");
                } else {
                    assert_eq!(
                        serde_json::to_value(typed.unwrap()).unwrap(),
                        step["value"],
                        "{name}"
                    );
                }
            }
        }
    }
    assert_eq!(count, 11);
    assert!(
        validator
            .validate_common_type("not_a_type", &Value::Null)
            .is_err()
    );
}

#[test]
fn official_wire_schema_subset() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/validator_v1_0.yaml")).unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/schema-coverage.json")).unwrap();
    let included: HashSet<&str> = manifest["validator_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    let excluded: HashSet<&str> = manifest["validator_excluded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(included.len() + excluded.len(), cases.len());
    let validator = validator();
    let mut passed = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        if !included.contains(name) {
            assert!(excluded.contains(name), "unclassified case {name}");
            continue;
        }
        passed += 1;
        for step in case["steps"].as_array().unwrap() {
            let results: Vec<_> = step["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| validator.validate_incoming(m))
                .collect();
            if let Some(expected) = step.get("expectError") {
                assert_eq!(
                    expected,
                    &serde_json::json!({"category":"ValidationError"}),
                    "unhandled assertion {name}"
                );
                assert!(
                    results.iter().any(Result::is_err),
                    "{name}: expected rejection"
                );
            } else {
                assert!(results.iter().all(Result::is_ok), "{name}: {results:?}");
            }
        }
    }
    assert_eq!(passed, included.len());
}
