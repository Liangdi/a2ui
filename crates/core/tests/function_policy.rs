//! Native declaration policies apply to every synchronous invocation path.
use a2ui_base::{
    catalog::{
        basic_functions::FormatStringFunction,
        function_api::{FunctionImplementation, ReturnType},
    },
    error::A2uiError,
    model::{data_context::DataContext, data_model::DataModel},
};
use serde_json::{Value, json};
use std::collections::HashMap;
struct Guarded;
impl FunctionImplementation for Guarded {
    fn name(&self) -> &'static str {
        "guarded"
    }
    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }
    fn validate_args(&self, args: &HashMap<String, Value>) -> Result<(), A2uiError> {
        if args.get("value") == Some(&json!("allowed")) {
            Ok(())
        } else {
            Err(A2uiError::Validation("native argument denied".into()))
        }
    }
    fn prepare_args(&self, args: &mut HashMap<String, Value>) -> Result<(), A2uiError> {
        args.entry("value".into()).or_insert(json!("allowed"));
        self.validate_args(args)
    }
    fn execute(&self, args: &HashMap<String, Value>, _: &DataContext) -> Result<Value, A2uiError> {
        Ok(args.get("value").cloned().unwrap_or(Value::Null))
    }
}
#[test]
fn native_defaults_and_validation_apply_to_direct_nested_and_template_calls() {
    let data = DataModel::new();
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = HashMap::from([
        (
            "guarded".into(),
            Box::new(Guarded) as Box<dyn FunctionImplementation>,
        ),
        (
            "formatString".into(),
            Box::new(FormatStringFunction) as Box<dyn FunctionImplementation>,
        ),
    ]);
    let context = DataContext::new(&data, &functions).with_current_protocol();
    for expression in [
        json!({"@call":"guarded"}),
        json!({"@call":"formatString","args":{"value":{"@call":"guarded"}}}),
        json!({"@call":"formatString","args":{"value":"${guarded()}"}}),
    ] {
        assert_eq!(
            context.resolve_dynamic_json(&expression).unwrap(),
            json!("allowed")
        );
    }
    for expression in [
        json!({"@call":"guarded","args":{"value":"bad"}}),
        json!({"@call":"formatString","args":{"value":{"@call":"guarded","args":{"value":"bad"}}}}),
        json!({"@call":"formatString","args":{"value":"${guarded(value: 'bad')}"}}),
    ] {
        assert!(
            context
                .resolve_dynamic_json(&expression)
                .unwrap_err()
                .to_string()
                .contains("native argument denied")
        );
    }
    assert_eq!(
        context.call_function_by_name("guarded", &HashMap::new()),
        Some(json!("allowed"))
    );
    assert!(
        context
            .call_function_by_name("guarded", &HashMap::from([("value".into(), json!("bad"))]))
            .is_none()
    );
}
