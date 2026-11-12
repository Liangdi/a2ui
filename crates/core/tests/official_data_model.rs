//! Runs the unmodified official shared data-model corpus against the Rust model.
use a2ui_base::model::data_model::DataModel;
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[test]
fn official_data_model_corpus() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/data_model.yaml")).unwrap();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        assert_eq!(case["action"], "data_model", "{name}: unsupported action");
        let mut model = DataModel::from_value(
            case.get("initial")
                .cloned()
                .unwrap_or(serde_json::json!({})),
        );
        let notified = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut subscriptions = Vec::new();
        for path in case["watch"].as_array().into_iter().flatten() {
            let path = path.as_str().unwrap().to_string();
            let events = notified.clone();
            let label = path.clone();
            subscriptions.push(
                model
                    .subscribe_path(&path, move |_| events.lock().unwrap().push(label.clone()))
                    .unwrap(),
            );
        }
        let mut input = Value::Null;
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            notified.lock().unwrap().clear();
            let path = step["path"].as_str().unwrap_or("/");
            let outcome = match step["op"].as_str().unwrap() {
                "get" => model.try_get(path).map(|v| v.cloned()),
                "set" => {
                    input = step["value"].clone();
                    model.try_set(path, input.clone()).map(|()| None)
                }
                "delete" => model.try_set(path, Value::Null).map(|()| None),
                "dispose" => {
                    subscriptions.clear();
                    Ok(None)
                }
                "mutate_input" => {
                    if let Some(slot) = input.pointer_mut(path) {
                        *slot = step["value"].clone();
                    }
                    Ok(None)
                }
                op => panic!("{name}: unsupported op {op}"),
            };
            if let Some(error) = step.get("expect_error") {
                let actual = outcome.expect_err(&format!("{name} step {index}"));
                assert_eq!(error["category"], "DataError");
                if let Some(message) = error["message"].as_str() {
                    assert!(actual.contains(message), "{name}: {actual}");
                }
            } else {
                let actual = outcome.unwrap_or_else(|e| panic!("{name} step {index}: {e}"));
                if let Some(expected) = step.get("expect") {
                    assert_eq!(actual.as_ref(), Some(expected), "{name} step {index}");
                }
                if step["expect_absent"] == true {
                    assert!(actual.is_none(), "{name} step {index}: {actual:?}");
                }
                if let Some(kind) = step["expect_type"].as_str() {
                    let actual = actual.unwrap();
                    assert!(
                        match kind {
                            "list" => actual.is_array(),
                            "object" => actual.is_object(),
                            _ => panic!("unknown type"),
                        },
                        "{name} step {index}"
                    );
                }
            }
            if let Some(expected) = step["expect_notified"].as_array() {
                let mut expected: Vec<_> = expected
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                expected.sort();
                let mut actual = notified.lock().unwrap().clone();
                actual.sort();
                assert_eq!(actual, expected, "{name} step {index}");
            }
            if let Some(values) = step["expect_values"].as_object() {
                for (path, expected) in values {
                    assert_eq!(model.get(path), Some(expected), "{name} step {index}");
                }
            }
        }
        eprintln!("[PASS] {name}");
    }
    eprintln!(
        "{} official data-model cases passed; none skipped",
        cases.len()
    );
}
