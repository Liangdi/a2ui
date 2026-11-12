use a2ui_base::{
    catalog::{basic_functions::FormatStringFunction, function_api::FunctionImplementation},
    model::{data_context::DataContext, data_model::DataModel},
};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn self_referential_template_stops_and_context_can_be_reused() {
    let data = DataModel::from_value(json!({"template":"${formatString(value: /template)}"}));
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = HashMap::from([(
        "formatString".into(),
        Box::new(FormatStringFunction) as Box<dyn FunctionImplementation>,
    )]);
    let context = DataContext::new(&data, &functions).with_current_protocol();
    let error = context
        .resolve_dynamic_json(
            &json!({"@call":"formatString","args":{"value":{"@path":"/template"}}}),
        )
        .unwrap_err();
    assert!(error.to_string().contains("budget exceeded"), "{error}");
    assert_eq!(
        context
            .resolve_dynamic_json(&json!({"@call":"formatString","args":{"value":"still usable"}}))
            .unwrap(),
        json!("still usable")
    );
}

#[test]
fn direct_native_template_and_context_clones_share_budget() {
    let data = DataModel::from_value(json!({"loop":"${formatString(value: /loop)}"}));
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = HashMap::from([(
        "formatString".into(),
        Box::new(FormatStringFunction) as Box<dyn FunctionImplementation>,
    )]);
    let context = DataContext::new(&data, &functions)
        .with_current_protocol()
        .nested("rows/0");
    let args = HashMap::from([("value".into(), json!("${formatString(value: /loop)}"))]);
    assert!(
        FormatStringFunction
            .execute(&args, &context.clone())
            .unwrap_err()
            .to_string()
            .contains("budget exceeded")
    );
}

#[test]
fn wide_template_cannot_reset_budget_at_each_interpolation() {
    let data = DataModel::new();
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = HashMap::from([(
        "formatString".into(),
        Box::new(FormatStringFunction) as Box<dyn FunctionImplementation>,
    )]);
    let context = DataContext::new(&data, &functions).with_current_protocol();
    let args = HashMap::from([(
        "value".into(),
        json!("${formatString(value: 'x')}".repeat(4097)),
    )]);
    assert!(
        FormatStringFunction
            .execute(&args, &context)
            .unwrap_err()
            .to_string()
            .contains("budget exceeded")
    );
}
