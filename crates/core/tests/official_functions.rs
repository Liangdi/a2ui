//! Pinned official function corpus. This is a scoped audit, not full conformance.
use a2ui_base::{
    catalog::{basic_functions::build_basic_functions, function_api::FunctionImplementation},
    model::{data_context::DataContext, data_model::DataModel},
};
use serde_json::{Value, json};
use std::collections::HashMap;

#[test]
fn official_function_corpus() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/functions.yaml")).unwrap();
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().into(), f))
        .collect();
    let data = DataModel::new();
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/functions-coverage.json")).unwrap();
    let mut known: HashMap<&str, &Value> = manifest["known_divergences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["name"].as_str().unwrap(), v))
        .collect();
    let mut omitted: HashMap<&str, &Value> = manifest["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["name"].as_str().unwrap(), v))
        .collect();
    let mut passed = 0;
    let mut unsupported = 0;
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        if case["action"] != "evaluate_function" || case.get("surface").is_some() {
            assert!(
                omitted.remove(name).is_some(),
                "undocumented unsupported case: {name}"
            );
            unsupported += 1;
            eprintln!(
                "UNSUPPORTED {name}: surface/validation adapter not implemented in function harness"
            );
            continue;
        }
        let mut context = DataContext::new(&data, &functions);
        if case["protocolVersion"] == "1.0" {
            context = context.with_current_protocol();
        }
        let args = case["args"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let result = functions[case["function"].as_str().unwrap()].execute(&args, &context);
        let valid = match &result {
            Ok(value) => case.get("expectError").is_none() && *value == case["expect"],
            Err(error) => {
                case.get("expectError").is_some()
                    && case["expectError"]["message"]
                        .as_str()
                        .is_none_or(|text| error.to_string().contains(text))
            }
        };
        if valid {
            passed += 1;
        } else {
            failures.push(json!({"name":name,"actual":format!("{result:?}"),"expected":case.get("expect").or(case.get("expectError"))}));
        }
    }
    eprintln!(
        "official functions: {passed} passed, {} diverged, {unsupported} unsupported of {}",
        failures.len(),
        cases.len()
    );
    for failure in &failures {
        eprintln!("DIVERGENCE {failure}");
    }
    for failure in &failures {
        let name = failure["name"].as_str().unwrap();
        let documented = known
            .remove(name)
            .unwrap_or_else(|| panic!("new conformance divergence: {failure}"));
        assert_eq!(
            failure["actual"], documented["observed"],
            "divergence changed: {name}"
        );
    }
    assert!(
        known.is_empty(),
        "fixed divergences must be removed from manifest: {:?}",
        known.keys()
    );
    assert!(
        omitted.is_empty(),
        "stale unsupported cases: {:?}",
        omitted.keys()
    );
}

#[test]
fn released_validators_return_structured_results_and_draft_stays_boolean() {
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().into(), f))
        .collect();
    let data = DataModel::new();
    for (name, args, valid) in [
        ("required", json!({"value":""}), false),
        ("regex", json!({"value":"abc","pattern":"^a"}), true),
        ("length", json!({"value":[1,2],"min":2}), true),
        ("numeric", json!({"value":"NaN"}), false),
        ("email", json!({"value":"a@b.c"}), false),
    ] {
        let args = args
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let legacy = DataContext::new(&data, &functions);
        let current = DataContext::new(&data, &functions).with_current_protocol();
        assert_eq!(
            functions[name].execute(&args, &legacy).unwrap(),
            json!(valid)
        );
        assert_eq!(
            functions[name].execute(&args, &current).unwrap(),
            json!({"valid":valid})
        );
        assert_eq!(
            functions[name]
                .execute(&args, &current.nested("rows/0"))
                .unwrap(),
            json!({"valid":valid})
        );
    }
}
