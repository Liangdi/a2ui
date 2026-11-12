//! Regression cases for the published 1.0 wire schema, not the July draft.
use std::collections::HashMap;

use a2ui_base::catalog::{
    Catalog,
    function_api::{AllowedCallers, FunctionImplementation, ReturnType},
};
use a2ui_base::error::A2uiError;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::data_context::DataContext;
use a2ui_base::protocol::rpc::{FunctionOutcome, FunctionResponse};
use serde_json::{Value, json};

struct Echo {
    allowed: AllowedCallers,
    activation: bool,
}
impl FunctionImplementation for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }
    fn return_type(&self) -> ReturnType {
        ReturnType::Any
    }
    fn allowed_callers(&self) -> AllowedCallers {
        self.allowed
    }
    fn requires_user_activation(&self) -> bool {
        self.activation
    }
    fn execute(&self, args: &HashMap<String, Value>, _: &DataContext) -> Result<Value, A2uiError> {
        Ok(args.get("value").cloned().unwrap_or(Value::Null))
    }
}
fn processor(allowed: AllowedCallers, activation: bool) -> MessageProcessor {
    MessageProcessor::new(vec![Catalog::new("test").with_function(Box::new(Echo {
        allowed,
        activation,
    }))])
}
fn apply(proc: &mut MessageProcessor, value: Value) {
    proc.process_message(MessageProcessor::parse_message(&value.to_string()).unwrap())
        .unwrap();
}
fn rpc(surface: Option<&str>, value: Value) -> Value {
    let mut request = json!({"version":"1.0","callRendererFunction":{
        "functionCallId":"call-1","callFunction":{"@call":"echo","catalogId":"test","args":{"value":value}}
    }});
    if let Some(id) = surface {
        request["callRendererFunction"]["surfaceId"] = json!(id);
    }
    request
}
#[test]
fn current_rpc_resolves_the_requested_surface_and_returns_current_envelope() {
    let mut proc = processor(AllowedCallers::RendererOrAgent, false);
    apply(
        &mut proc,
        json!({"version":"1.0","createSurface":{"surfaceId":"one","dataModel":{"name":"First"}}}),
    );
    apply(
        &mut proc,
        json!({"version":"1.0","createSurface":{"surfaceId":"two","dataModel":{"name":"Second"}}}),
    );
    apply(&mut proc, rpc(Some("two"), json!({"@path":"/name"})));
    assert_eq!(
        serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap(),
        json!({
            "version":"1.0","rendererFunctionResponse":{"functionCallId":"call-1","value":"Second"}
        })
    );
}
#[test]
fn surfaceless_rpc_does_not_borrow_an_unrelated_surface() {
    let mut proc = processor(AllowedCallers::AgentOnly, false);
    apply(
        &mut proc,
        json!({"version":"1.0","createSurface":{"surfaceId":"one","dataModel":{"secret":"not shared"}}}),
    );
    apply(&mut proc, rpc(None, json!({"@path":"/secret"})));
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(message["rendererFunctionResponse"]["value"], Value::Null);
}
#[test]
fn renderer_only_and_user_activation_functions_reject_agent_calls() {
    for (allowed, activation) in [
        (AllowedCallers::RendererOnly, false),
        (AllowedCallers::RendererOrAgent, true),
    ] {
        let mut proc = processor(allowed, activation);
        apply(&mut proc, rpc(None, json!("no")));
        let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
        assert_eq!(
            message["rendererFunctionResponse"]["error"]["code"],
            "INVALID_FUNCTION_CALL"
        );
    }
}
#[test]
fn catalog_lookup_never_falls_back_to_an_arbitrary_catalog() {
    let mut proc = processor(AllowedCallers::RendererOrAgent, false);
    let mut request = rpc(None, json!("no"));
    request["callRendererFunction"]["callFunction"]["catalogId"] = json!("not-registered");
    apply(&mut proc, request);
    assert!(serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap()["rendererFunctionResponse"]["error"].is_object());
}
#[test]
fn current_update_requires_value_and_preserves_explicit_null() {
    let missing = json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/name"}});
    assert!(MessageProcessor::parse_message(&missing.to_string()).is_err());
    let mut proc = processor(AllowedCallers::RendererOnly, false);
    apply(
        &mut proc,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","dataModel":{"name":"remove me"}}}),
    );
    let deletion =
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/name","value":null}});
    let parsed = MessageProcessor::parse_message(&deletion.to_string()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), deletion);
    proc.process_message(parsed).unwrap();
    assert!(
        proc.model
            .get_surface("s")
            .unwrap()
            .data_model
            .borrow()
            .get("/name")
            .is_none()
    );
}
#[test]
fn function_response_requires_exactly_one_result_including_null() {
    let null_result = json!({"functionCallId":"x","value":null});
    let response: FunctionResponse = serde_json::from_value(null_result.clone()).unwrap();
    assert!(matches!(response.outcome, FunctionOutcome::Success(_)));
    assert_eq!(serde_json::to_value(response).unwrap(), null_result);
    for bad in [
        json!({"functionCallId":"x"}),
        json!({"functionCallId":"x","value":null,"error":{"code":"bad","message":"bad"}}),
    ] {
        assert!(serde_json::from_value::<FunctionResponse>(bad).is_err());
    }
}
#[test]
fn agent_response_is_available_to_the_host() {
    let mut proc = processor(AllowedCallers::RendererOnly, false);
    apply(
        &mut proc,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":"a","value":{"ok":true}}}),
    );
    assert_eq!(proc.drain_agent_function_responses().len(), 1);
    assert!(proc.drain_agent_function_responses().is_empty());
}
#[test]
fn rejects_ambiguous_current_envelopes_and_reserved_surface_component() {
    for invalid in [
        json!({"version":"1.0","deleteSurface":{"surfaceId":"x"},"createSurface":{"surfaceId":"y"}}),
        json!({"version":"1.0","createSurface":{"surfaceId":"x","components":[{"id":"root","component":"Surface"}]}}),
        json!({"version":"1.0","createSurface":{"surfaceId":"x","theme":{}}}),
        json!({"version":"1.0","updateComponents":{"surfaceId":"x","components":[]}}),
    ] {
        assert!(MessageProcessor::parse_message(&invalid.to_string()).is_err());
    }
}

#[test]
fn nested_agent_calls_cannot_bypass_native_caller_permissions() {
    let mut proc = MessageProcessor::new(vec![
        Catalog::new("test")
            .with_function(Box::new(Echo {
                allowed: AllowedCallers::RendererOrAgent,
                activation: false,
            }))
            .with_function(Box::new(
                a2ui_base::catalog::basic_functions::RequiredFunction,
            )),
    ]);
    apply(
        &mut proc,
        rpc(None, json!({"@call":"required","args":{"value":"x"}})),
    );
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(
        message["rendererFunctionResponse"]["error"]["code"],
        "INVALID_FUNCTION_CALL"
    );
}

#[test]
fn legacy_function_normalization_never_rewrites_the_data_model() {
    let raw = json!({"version":"v1.0","createSurface":{"surfaceId":"s","catalogId":"test","dataModel":{"call":"ordinary","args":{"value":{"path":"/literal"}}}}});
    let parsed = MessageProcessor::parse_message(&raw.to_string()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap()["createSurface"]["dataModel"],
        raw["createSurface"]["dataModel"]
    );
}
