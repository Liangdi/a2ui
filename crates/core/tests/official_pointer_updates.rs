//! Unmodified current wire-level pointer/deletion corpora from a2ui-project/a2ui
//! revision eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41.
//! These exercise the real message processor, including typed DataError mapping.
use a2ui_base::{catalog::Catalog, error::A2uiError, message_processor::MessageProcessor};
use serde_json::Value;

fn run(source: &str, count: usize) {
    let cases: Vec<Value> = serde_yaml_ng::from_str(source).unwrap();
    assert_eq!(cases.len(), count);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        assert_eq!(case["action"], "validate", "{name}");
        assert_eq!(case["catalog"]["protocolVersion"], "1.0", "{name}");
        let mut processor = MessageProcessor::new(vec![Catalog::new("basic")]);
        for step in case["steps"].as_array().unwrap() {
            let mut errors = Vec::new();
            for message in step["payload"].as_array().unwrap() {
                if let Err(error) = processor.process_json_message(&message.to_string()) {
                    errors.push(error);
                }
            }
            if let Some(expected) = step.get("expectError") {
                assert_eq!(expected["category"], "DataError", "{name}");
                assert_eq!(errors.len(), 1, "{name}: {errors:?}");
                assert!(
                    matches!(errors[0], A2uiError::InvalidPointer(_)),
                    "{name}: {errors:?}"
                );
                assert!(
                    errors[0]
                        .to_string()
                        .contains(expected["message"].as_str().unwrap()),
                    "{name}: {errors:?}"
                );
            } else {
                assert!(errors.is_empty(), "{name}: {errors:?}");
            }
        }
        if let Some(surfaces) = case["expect"]["surfaces"].as_object() {
            for (id, expected) in surfaces {
                let surface = processor.model.get_surface(id).unwrap();
                assert_eq!(
                    surface.data_model.borrow().as_value(),
                    &expected["dataModel"],
                    "{name}"
                );
            }
        }
        eprintln!("[PASS] {name}");
    }
}
#[test]
fn official_pointer_updates() {
    run(include_str!("conformance/data_model_pointers.yaml"), 7);
}
#[test]
fn official_deletion_updates() {
    run(include_str!("conformance/data_deletion.yaml"), 7);
}
