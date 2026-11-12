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
        json!({"version":"1.0","createSurface":{"surfaceId":"s"}}),
    );
    proc.call_agent_function(
        serde_json::from_value(json!({
            "surfaceId":"s", "functionCallId":"a",
            "callFunction":{"@call":"remote", "catalogId":"test", "args":{}}
        }))
        .unwrap(),
    )
    .unwrap();
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
fn current_rpc_nested_renderer_only_builtins_execute_locally() {
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
        rpc(
            None,
            json!({"@call":"required","catalogId":"test","args":{"value":"x"}}),
        ),
    );
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(
        message["rendererFunctionResponse"]["value"],
        json!({"valid":true})
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

#[test]
fn passive_renderer_evaluation_cannot_activate_gesture_functions() {
    use a2ui_base::model::data_model::DataModel;
    let data = DataModel::new();
    let catalog = Catalog::new("test").with_function(Box::new(Echo {
        allowed: AllowedCallers::RendererOrAgent,
        activation: true,
    }));
    let context = DataContext::new(&data, &catalog.functions);
    assert!(
        context
            .resolve_dynamic_json(&json!({"@call":"echo","args":{"value":"no"}}))
            .is_err()
    );
}

#[test]
fn legacy_rpc_obeys_remote_caller_and_activation_rules() {
    for (allowed, activation) in [
        (AllowedCallers::RendererOnly, false),
        (AllowedCallers::RendererOrAgent, true),
    ] {
        let mut proc = processor(allowed, activation);
        apply(
            &mut proc,
            json!({"version":"v1.0","functionCallId":"legacy",
            "wantResponse":true,"callFunction":{"call":"echo","args":{"value":"no"}}}),
        );
        let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
        assert_eq!(message["error"]["code"], "INVALID_FUNCTION_CALL");
    }
}

#[test]
fn legacy_rpc_rejects_forbidden_nested_calls() {
    let mut proc = MessageProcessor::new(vec![
        Catalog::new("remote")
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
        json!({"version":"v1.0","functionCallId":"legacy",
        "wantResponse":true,"callFunction":{"call":"echo","catalogId":"remote","args":{
            "value":{"@call":"required","args":{"value":"no"}}
        }}}),
    );
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(message["error"]["code"], "INVALID_FUNCTION_CALL");
}

#[test]
fn authorized_legacy_rpc_still_returns_a_legacy_response() {
    let mut proc = processor(AllowedCallers::RendererOrAgent, false);
    apply(
        &mut proc,
        json!({"version":"v1.0","functionCallId":"legacy",
        "wantResponse":true,"callFunction":{"call":"echo","args":{"value":42}}}),
    );
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(message["functionResponse"]["value"], 42);
}

#[test]
fn legacy_event_context_paths_bind_in_template_scope_only_for_draft_messages() {
    use a2ui_base::model::data_model::DataModel;
    let data = DataModel::from_value(json!({"rows":[{"name":"first"},{"name":"second"}]}));
    let functions = HashMap::new();
    let root = DataContext::new(&data, &functions);
    let context = root.nested("rows/1");
    for payload in ["createSurface", "updateComponents"] {
        for version in ["v1.0", "1.0"] {
            let raw = json!({"version":version, payload:{"surfaceId":"s","components":[{
                "id":"button","component":"Button","action":{"event":{"name":"choose",
                    "context":{"name":{"path":"name"},"nested":[{"path":"name"}]}
                }}
            }]}});
            let parsed = MessageProcessor::parse_message(&raw.to_string()).unwrap();
            let normalized = serde_json::to_value(parsed).unwrap();
            let values = &normalized[payload]["components"][0]["action"]["event"]["context"];
            let resolved = context.resolve_dynamic_json(values).unwrap();
            if version == "v1.0" {
                assert_eq!(resolved, json!({"name":"second","nested":["second"]}));
            } else {
                assert_eq!(
                    resolved,
                    json!({"name":{"path":"name"},"nested":[{"path":"name"}]})
                );
            }
        }
    }
}

#[test]
fn typed_processing_normalizes_draft_event_context_without_parse_message() {
    for version in ["v1.0", "1.0"] {
        let mut proc = processor(AllowedCallers::RendererOnly, false);
        let message = serde_json::from_value(json!({"version":version,"createSurface":{
            "surfaceId":"s","dataModel":{"rows":[{"name":"first"},{"name":"second"}]},
            "components":[{"id":"root","component":"Button","action":{"event":{
                "name":"choose","context":{"name":{"path":"name"}}
            }}}]
        }}))
        .unwrap();
        proc.process_message(message).unwrap();
        let surface = proc.model.get_surface("s").unwrap();
        let components = surface.components.borrow();
        let button = components.get("root").unwrap();
        let data = surface.data_model.borrow();
        let functions = HashMap::new();
        let context = DataContext::new(&data, &functions).nested("rows/1");
        let result = context
            .resolve_dynamic_json(&button.properties["action"]["event"]["context"])
            .unwrap();
        assert_eq!(
            result,
            if version == "v1.0" {
                json!({"name":"second"})
            } else {
                json!({"name":{"path":"name"}})
            }
        );
    }
}

#[test]
fn incoming_rpc_applies_native_argument_defaults_before_execution() {
    struct WithDefault;
    impl FunctionImplementation for WithDefault {
        fn name(&self) -> &'static str {
            "defaults"
        }
        fn return_type(&self) -> ReturnType {
            ReturnType::String
        }
        fn allowed_callers(&self) -> AllowedCallers {
            AllowedCallers::RendererOrAgent
        }
        fn prepare_args(&self, args: &mut HashMap<String, Value>) -> Result<(), A2uiError> {
            args.entry("value".into()).or_insert(json!("default"));
            Ok(())
        }
        fn execute(
            &self,
            args: &HashMap<String, Value>,
            _: &DataContext,
        ) -> Result<Value, A2uiError> {
            Ok(args["value"].clone())
        }
    }
    let mut p = MessageProcessor::new(vec![
        Catalog::new("test").with_function(Box::new(WithDefault)),
    ]);
    apply(
        &mut p,
        json!({"version":"1.0","callRendererFunction":{
            "functionCallId":"defaults","callFunction":{"@call":"defaults","catalogId":"test"}
        }}),
    );
    let response = serde_json::to_value(p.drain_outgoing().remove(0)).unwrap();
    assert_eq!(response["rendererFunctionResponse"]["value"], "default");
}

#[test]
fn legacy_rpc_preserves_legacy_function_context_and_return_type() {
    struct Versioned;
    impl FunctionImplementation for Versioned {
        fn name(&self) -> &'static str {
            "versioned"
        }
        fn return_type(&self) -> ReturnType {
            ReturnType::Boolean
        }
        fn return_type_for_protocol(&self, current: bool) -> ReturnType {
            if current {
                ReturnType::Object
            } else {
                ReturnType::Boolean
            }
        }
        fn allowed_callers(&self) -> AllowedCallers {
            AllowedCallers::RendererOrAgent
        }
        fn execute(
            &self,
            _: &HashMap<String, Value>,
            context: &DataContext,
        ) -> Result<Value, A2uiError> {
            Ok(if context.is_current_protocol() {
                json!({"valid":true})
            } else {
                json!(true)
            })
        }
    }
    let mut p = MessageProcessor::new(vec![
        Catalog::new("test").with_function(Box::new(Versioned)),
    ]);
    apply(
        &mut p,
        json!({"version":"v1.0","callFunction":{"call":"versioned","catalogId":"test"},"functionCallId":"legacy","wantResponse":true}),
    );
    let response = serde_json::to_value(p.drain_outgoing().remove(0)).unwrap();
    assert_eq!(response["functionResponse"]["value"], true);
    apply(
        &mut p,
        json!({"version":"1.0","callRendererFunction":{"functionCallId":"current","callFunction":{"@call":"versioned","catalogId":"test"}}}),
    );
    let response = serde_json::to_value(p.drain_outgoing().remove(0)).unwrap();
    assert_eq!(
        response["rendererFunctionResponse"]["value"],
        json!({"valid":true})
    );
}

#[test]
fn current_rpc_nested_arguments_reject_agent_only_activation_and_remote_fallback() {
    for (allowed, activation) in [
        (AllowedCallers::AgentOnly, false),
        (AllowedCallers::RendererOrAgent, true),
    ] {
        let mut proc = MessageProcessor::new(vec![
            Catalog::new("test").with_function(Box::new(Echo {
                allowed: AllowedCallers::RendererOrAgent,
                activation: false,
            })),
            Catalog::new("nested").with_function(Box::new(Echo {
                allowed,
                activation,
            })),
        ]);
        apply(
            &mut proc,
            rpc(
                None,
                json!({"@call":"echo","catalogId":"nested","args":{"value":"unsafe"}}),
            ),
        );
        let outgoing = proc.drain_outgoing();
        assert_eq!(outgoing.len(), 1);
        let message = serde_json::to_value(&outgoing[0]).unwrap();
        assert_eq!(
            message["rendererFunctionResponse"]["error"]["code"],
            "EXECUTION_ERROR"
        );
    }
    let mut proc = processor(AllowedCallers::RendererOrAgent, false);
    apply(
        &mut proc,
        rpc(
            None,
            json!({"@call":"missing","catalogId":"test","args":{}}),
        ),
    );
    let outgoing = proc.drain_outgoing();
    assert_eq!(
        outgoing.len(),
        1,
        "nested expression must not queue remote fallback"
    );
    let message = serde_json::to_value(&outgoing[0]).unwrap();
    assert_eq!(
        message["rendererFunctionResponse"]["error"]["code"],
        "EXECUTION_ERROR"
    );
}

#[test]
fn malformed_current_rpc_queues_correlated_error() {
    let mut proc = processor(AllowedCallers::RendererOrAgent, false);
    for call in [
        json!({"functionCallId":"bad"}),
        json!({"functionCallId":"bad","callFunction":{"@call":"echo","catalogId":"test","args":[]}}),
    ] {
        assert!(
            proc.process_json_message(
                &json!({"version":"1.0","callRendererFunction":call}).to_string()
            )
            .is_err()
        );
        let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
        assert_eq!(message["error"]["functionCallId"], "bad");
        assert_eq!(message["error"]["code"], "INVALID_FUNCTION_CALL");
    }
}

#[test]
fn incoming_activation_requires_explicit_trusted_host_entry_point() {
    let mut proc = processor(AllowedCallers::RendererOrAgent, true);
    let request = rpc(None, json!("activated"));
    apply(&mut proc, request.clone());
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(
        message["rendererFunctionResponse"]["error"]["code"],
        "INVALID_FUNCTION_CALL"
    );
    let call = serde_json::from_value(request["callRendererFunction"].clone()).unwrap();
    proc.process_renderer_call_from_user_gesture(&call).unwrap();
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(message["rendererFunctionResponse"]["value"], "activated");
}

#[test]
fn native_catalog_protocol_identity_distinguishes_draft_and_release() {
    for version in ["1.0", "v1.0", "v0.9"] {
        let mut proc = MessageProcessor::new(vec![]);
        proc.register_native_catalog(
            Catalog::new("test").with_function(Box::new(Echo {
                allowed: AllowedCallers::RendererOrAgent,
                activation: false,
            })),
            version,
        )
        .unwrap();
        apply(&mut proc, rpc(None, json!("ok")));
        let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
        if version == "1.0" {
            assert_eq!(message["rendererFunctionResponse"]["value"], "ok");
        } else {
            assert!(
                message["rendererFunctionResponse"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("protocol version")
            );
        }
        apply(
            &mut proc,
            json!({"version":"v1.0","functionCallId":"draft","wantResponse":true,"callFunction":{"call":"echo","catalogId":"test","args":{"value":"ok"}}}),
        );
        let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
        if version == "v1.0" {
            assert_eq!(message["functionResponse"]["value"], "ok", "{message}");
        } else {
            assert!(
                message["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("protocol version")
            );
        }
    }
}

#[test]
fn inline_draft_catalog_keeps_exact_version_identity() {
    let mut proc = MessageProcessor::new(vec![]);
    proc.register_inline_catalog(json!({"catalogId":"test","protocolVersion":"v1.0","components":{},"functions":{"echo":{"returnType":"any","allowedCallers":"rendererOrAgent"}}})).unwrap();
    apply(&mut proc, rpc(None, json!("wrong version")));
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert!(
        message["rendererFunctionResponse"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("protocol version")
    );
    apply(
        &mut proc,
        json!({"version":"v1.0","functionCallId":"draft","wantResponse":true,"callFunction":{"call":"echo","catalogId":"test","args":{}}}),
    );
    let message = serde_json::to_value(proc.drain_outgoing().pop().unwrap()).unwrap();
    assert!(
        !message["error"]["message"]
            .as_str()
            .unwrap()
            .contains("protocol version"),
        "{message}"
    );
}

#[test]
fn registered_current_catalog_resolves_common_argument_schema_refs() {
    use a2ui_base::message_processor::RendererCall;
    let mut catalog: Value = serde_json::from_str(include_str!(
        "conformance/catalog-definitions/function_metadata_catalog_v1_0.json"
    ))
    .unwrap();
    catalog["functions"]["launch"]["requiresUserActivation"] = json!(false);
    let id = catalog["catalogId"].as_str().unwrap().to_owned();
    let mut proc = MessageProcessor::new(vec![]);
    proc.register_inline_catalog(catalog).unwrap();
    apply(
        &mut proc,
        json!({"version":"1.0","createSurface":{"surfaceId":"schema","catalogId":id}}),
    );
    let call = |url| serde_json::from_value(json!({"@call":"launch","args":{"url":url}})).unwrap();
    assert!(matches!(
        proc.prepare_renderer_call("schema", "", call(json!("https://example.com")), false)
            .unwrap(),
        RendererCall::Remote(_)
    ));
    assert!(
        proc.prepare_renderer_call("schema", "", call(json!(123)), false)
            .is_err()
    );
}
