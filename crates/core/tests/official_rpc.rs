//! Pinned official RPC corpus, using the same native mocks as the Python runner.
//!
//! The corpus is consumed unchanged. Cases that require the Python SDK's mounted
//! NodeResolver/action-result API are explicitly recorded in rpc-coverage.json;
//! they must not be counted as passing merely because an expression test passes.
use a2ui_base::{
    catalog::{
        Catalog, FunctionImplementation, SchemaOnlyFunction,
        basic_functions::build_basic_functions,
        function_api::{AllowedCallers, ReturnType},
    },
    error::A2uiError,
    message_processor::MessageProcessor,
    model::data_context::DataContext,
    protocol::rpc::CallAgentFunction,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

type TestResult<T = ()> = Result<T, String>;

struct MockFunction {
    policy: SchemaOnlyFunction,
}
impl FunctionImplementation for MockFunction {
    fn name(&self) -> &'static str {
        self.policy.name()
    }
    fn return_type(&self) -> ReturnType {
        self.policy.return_type()
    }
    fn allowed_callers(&self) -> AllowedCallers {
        self.policy.allowed_callers()
    }
    fn requires_user_activation(&self) -> bool {
        self.policy.requires_user_activation()
    }
    fn prepare_args(&self, args: &mut HashMap<String, Value>) -> Result<(), A2uiError> {
        self.policy.prepare_args(args)
    }
    fn execute(&self, args: &HashMap<String, Value>, _: &DataContext) -> Result<Value, A2uiError> {
        // Matches _make_mock_rpc_executor in the pinned Python test runner.
        Ok(match self.name() {
            "playMedia" => json!({"playing": true, "timestamp": 0}),
            "openExternalUrl" => json!({"opened": true}),
            "failingFunction" => {
                return Err(A2uiError::InvalidFunctionCall(
                    "An error occurred during function execution.".into(),
                ));
            }
            "calculateTax" => {
                json!(args.get("amount").and_then(Value::as_f64).unwrap_or(0.0) * 0.1)
            }
            "echoArgs" => serde_json::to_value(args)?,
            _ => Value::Null,
        })
    }
}

fn field<'a>(value: &'a Value, key: &str) -> TestResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string {key} in {value}"))
}
fn keys(value: &Value, allowed: &[&str], location: &str) -> TestResult {
    for key in value
        .as_object()
        .ok_or_else(|| format!("{location} must be object"))?
        .keys()
    {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unhandled {location} key: {key}"));
        }
    }
    Ok(())
}
fn subset(expected: &Value, actual: &Value) -> TestResult {
    if let Some(object) = expected.as_object() {
        for (key, value) in object {
            let actual_value = actual
                .get(key)
                .ok_or_else(|| format!("missing {key} in {actual}"))?;
            if key == "message" {
                if !actual_value
                    .as_str()
                    .unwrap_or("")
                    .contains(value.as_str().ok_or("expected message must be string")?)
                {
                    return Err(format!(
                        "expected message containing {value}, got {actual_value}"
                    ));
                }
            } else {
                subset(value, actual_value)?;
            }
        }
        Ok(())
    } else if expected == actual {
        Ok(())
    } else {
        Err(format!("expected {expected}, got {actual}"))
    }
}
fn send(p: &mut MessageProcessor, value: &Value) -> TestResult {
    p.process_json_message(&value.to_string())
        .map_err(|e| e.to_string())
}
fn outgoing(p: &mut MessageProcessor) -> Vec<Value> {
    p.drain_outgoing()
        .into_iter()
        .map(|v| serde_json::to_value(v).unwrap())
        .collect()
}
fn catalog_info(case: &Value) -> (String, String) {
    let args = &case["args"];
    let incoming_catalog = if case
        .pointer("/expect/response/rendererFunctionResponse/error/message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .contains("Catalog not found")
    {
        None
    } else {
        args.pointer("/message/callRendererFunction/callFunction/catalogId")
            .and_then(Value::as_str)
    };
    let id = args["catalogId"]
        .as_str()
        .or(incoming_catalog)
        .or_else(|| {
            args.pointer("/outboundCall/callFunction/catalogId")
                .and_then(Value::as_str)
        })
        .unwrap_or("basic");
    let version = args["catalogVersion"]
        .as_str()
        .or(case["catalog"]["protocolVersion"].as_str())
        .or(case["protocolVersion"].as_str())
        .unwrap_or("1.0");
    (id.into(), version.into())
}
fn processor(case: &Value) -> TestResult<MessageProcessor> {
    let args = &case["args"];
    let (id, version) = catalog_info(case);
    let metadata = args.get("functionMetadata").cloned().unwrap_or(json!({}));
    let mut catalog = Catalog::new(&id);
    for (name, meta) in metadata
        .as_object()
        .ok_or("functionMetadata must be object")?
    {
        keys(
            meta,
            &[
                "allowedCallers",
                "requiresUserActivation",
                "schema",
                "parameters",
                "returnType",
            ],
            "function metadata",
        )?;
        let callers = match meta["allowedCallers"].as_str().unwrap_or("rendererOrAgent") {
            "rendererOnly" => AllowedCallers::RendererOnly,
            "agentOnly" => AllowedCallers::AgentOnly,
            "rendererOrAgent" => AllowedCallers::RendererOrAgent,
            other => return Err(format!("unknown allowedCallers {other}")),
        };
        let policy = SchemaOnlyFunction::new(
            name.clone(),
            a2ui_base::catalog::schema_only::parse_return_type(
                meta["returnType"].as_str().unwrap_or("any"),
            ),
        )
        .with_policy(
            meta.get("schema")
                .or_else(|| meta.get("parameters"))
                .cloned()
                .unwrap_or(json!({"type":"object"})),
            callers,
            meta["requiresUserActivation"].as_bool().unwrap_or(false),
        )
        .map_err(|e| e.to_string())?;
        catalog = catalog.with_function(Box::new(MockFunction { policy }));
    }
    let mut catalogs = Vec::new();
    if id == "basic" {
        for function in build_basic_functions() {
            if !catalog.functions.contains_key(function.name()) {
                catalog = catalog.with_function(function);
            }
        }
        catalogs.push(catalog);
    } else {
        catalogs.push(catalog);
        let mut basic = Catalog::new("basic");
        for function in build_basic_functions() {
            basic = basic.with_function(function);
        }
        catalogs.push(basic);
    }
    let mut p = MessageProcessor::new(vec![]);
    // Explicit host-declared catalog protocol version, not a schema-only stand-in.
    for catalog in catalogs {
        p.register_native_catalog(catalog, &version)
            .map_err(|e| e.to_string())?;
    }
    if let Some(available) = args["outboundListener"].as_bool() {
        p.set_agent_transport_available(available);
    }
    if let Some(surface) = args["surfaceId"].as_str() {
        send(
            &mut p,
            &json!({"version":"1.0","createSurface":{"surfaceId":surface,"catalogId":id,"dataModel":args.get("dataModel").cloned().unwrap_or(json!({}))}}),
        )?;
    }
    // Rust's host RPC API deliberately requires an existing surface. The Python
    // test setup omits it for explicit outbound calls; supply its named scope.
    if let Some(surface) = args
        .pointer("/outboundCall/surfaceId")
        .and_then(Value::as_str)
    {
        if p.model.get_surface(surface).is_none() {
            send(
                &mut p,
                &json!({"version":"1.0","createSurface":{"surfaceId":surface,"catalogId":id}}),
            )?;
        }
    }
    Ok(p)
}
fn inbound(p: &mut MessageProcessor, case: &Value) -> TestResult {
    let args = &case["args"];
    let message = &args["message"];
    let expected = &case["expect"];
    let expected_error = case.get("expectError").or_else(|| expected.get("error"));
    if let Some(error) = expected_error {
        keys(
            error,
            &[
                "category",
                "code",
                "functionCallId",
                "message",
                "rendererError",
            ],
            "expectError",
        )?;
        let actual_error = p
            .process_json_message(&message.to_string())
            .expect_err("invalid wire payload must fail");
        if let Some(category) = error["category"].as_str() {
            if category != "ValidationError" {
                return Err(format!("unsupported expected category {category}"));
            }
            if !matches!(actual_error, A2uiError::Validation(_) | A2uiError::Parse(_)) {
                return Err(format!("expected validation error, got {actual_error}"));
            }
        }
        if let Some(text) = error["message"].as_str() {
            if !actual_error.to_string().contains(text) {
                return Err(format!(
                    "expected error containing {text}, got {actual_error}"
                ));
            }
        }
        let messages = outgoing(p);
        if let Some(wire) = error.get("rendererError") {
            if messages.len() != 1 {
                return Err(format!("expected one wire error, got {messages:?}"));
            }
            subset(wire, &messages[0])?;
        }
        if let Some(id) = error.get("functionCallId") {
            if messages.len() != 1 {
                return Err("correlated error was not emitted".into());
            }
            subset(&json!({"error":{"functionCallId":id}}), &messages[0])?;
        }
        if let Some(code) = error.get("code") {
            if messages.len() != 1 {
                return Err("coded error was not emitted".into());
            }
            subset(&json!({"error":{"code":code}}), &messages[0])?;
        }
        // Some fixtures provide both expect.error and expectError; check both.
        if let Some(expected) = expected.get("error") {
            if messages.len() != 1 {
                return Err("expected wire error was not emitted".into());
            }
            subset(expected, &messages[0]["error"])?;
        }
    } else if expected.get("response").is_some() {
        if args["userActivationPresent"] == true {
            let call = serde_json::from_value(message["callRendererFunction"].clone())
                .map_err(|e| e.to_string())?;
            p.process_renderer_call_from_user_gesture(&call)
                .map_err(|e| e.to_string())?;
        } else {
            send(p, message)?;
        }
        let messages = outgoing(p);
        if expected["response"].is_null() {
            if !messages.is_empty()
                || !p.drain_agent_call_results().is_empty()
                || !p.drain_agent_function_responses().is_empty()
            {
                return Err("unknown reply must be ignored".into());
            }
            if !p.drain_action_errors().is_empty() || !p.drain_evaluation_diagnostics().is_empty() {
                return Err("ignored reply produced diagnostics".into());
            }
        } else {
            if messages.len() != 1 {
                return Err(format!("expected one response, got {messages:?}"));
            }
            subset(&expected["response"], &messages[0])?;
            // Returned values are exact, not partial object matches.
            if let Some(value) = expected.pointer("/response/rendererFunctionResponse/value") {
                if &messages[0]["rendererFunctionResponse"]["value"] != value {
                    return Err(format!("returned value differs: {}", messages[0]));
                }
            }
        }
    } else {
        return Err("inbound case lacks an assertion".into());
    }
    Ok(())
}
fn begin(
    p: &mut MessageProcessor,
    value: &Value,
    now: Instant,
) -> Result<(), a2ui_base::message_processor::RpcCallError> {
    let mut wire = value.clone();
    wire.as_object_mut().unwrap().remove("timeoutMs");
    let call: CallAgentFunction =
        serde_json::from_value(wire).expect("valid fixture outbound call");
    p.begin_agent_call(
        call,
        value["timeoutMs"].as_u64().map(Duration::from_millis),
        now,
    )
}
fn completed(p: &mut MessageProcessor) -> TestResult<Value> {
    let results = p.drain_agent_call_results();
    if results.len() != 1 {
        return Err(format!("expected one completed call, got {results:?}"));
    }
    serde_json::to_value(&results[0]).map_err(|e| e.to_string())
}
fn outbound(p: &mut MessageProcessor, case: &Value) -> TestResult {
    let args = &case["args"];
    let call = &args["outboundCall"];
    let expected = case.get("expect").cloned().unwrap_or(json!({}));
    let expected_error = case.get("expectError").or_else(|| expected.get("error"));
    let now = Instant::now();
    if let Some(error) = expected_error {
        keys(error, &["code", "message"], "outbound expectError")?;
        let actual = if args["outboundListener"] == false {
            let error = begin(p, call, now).expect_err("no listener must reject call");
            if !outgoing(p).is_empty() {
                return Err("no-listener failure queued wire message".into());
            }
            json!({"code":error.code,"message":error.message})
        } else {
            begin(p, call, now).map_err(|e| e.to_string())?;
            let emitted = outgoing(p);
            if emitted.len() != 1 {
                return Err(format!("call did not emit exactly once: {emitted:?}"));
            }
            if let Some(second) = args.get("secondOutboundCall") {
                let e = begin(p, second, now).expect_err("duplicate pending id must be rejected");
                if !outgoing(p).is_empty() {
                    return Err("duplicate emitted extra message".into());
                }
                json!({"code":e.code,"message":e.message})
            } else if args["afterCall"] == "cancel" {
                p.cancel_surface_calls(field(call, "surfaceId")?);
                completed(p)?["error"].clone()
            } else if args["afterCall"] == "dispose" {
                p.dispose_agent_calls();
                let settled = completed(p)?;
                let in_flight = expected
                    .get("inFlightError")
                    .cloned()
                    .unwrap_or(json!({"code":"CANCELLED"}));
                subset(&in_flight, &settled["error"])?;
                let e = begin(p, call, now).expect_err("disposed RPC must reject new call");
                json!({"code":e.code,"message":e.message})
            } else if let Some(timeout) = call["timeoutMs"].as_u64() {
                p.expire_agent_calls(now + Duration::from_millis(timeout));
                completed(p)?["error"].clone()
            } else if let Some(reply) = args.get("inboundResponse") {
                send(p, reply)?;
                completed(p)?["error"].clone()
            } else {
                return Err("unhandled outbound error scenario".into());
            }
        };
        subset(error, &actual)?;
        if let Some(late) = args.get("lateInboundResponse") {
            if expected["lateResponseIgnored"] != true {
                return Err("late reply must have explicit ignore assertion".into());
            }
            if !outgoing(p).is_empty() {
                return Err("unexpected pre-late-response messages".into());
            }
            send(p, late)?;
            if !outgoing(p).is_empty()
                || !p.drain_agent_call_results().is_empty()
                || !p.drain_agent_function_responses().is_empty()
                || !p.drain_action_errors().is_empty()
                || !p.drain_evaluation_diagnostics().is_empty()
            {
                return Err("late reply changed settled state".into());
            }
        }
    } else {
        begin(p, call, now).map_err(|e| e.to_string())?;
        let messages = outgoing(p);
        if messages.len() != 1 {
            return Err("expected one outbound call".into());
        }
        subset(
            &json!({"version":"1.0","callAgentFunction":call}),
            &messages[0],
        )?;
        let reply = args
            .get("inboundResponse")
            .ok_or("success case needs inboundResponse")?;
        send(p, reply)?;
        let result = completed(p)?;
        if result["functionCallId"] != call["functionCallId"] {
            return Err("correlation id differs".into());
        }
        if let Some(id) = expected.get("correlatedCallId") {
            subset(id, &result["functionCallId"])?;
        }
        if result.get("value") != expected.get("result") {
            return Err(format!(
                "expected result {}, got {result}",
                expected["result"]
            ));
        }
    }
    Ok(())
}
fn run(case: &Value) -> TestResult {
    keys(
        case,
        &[
            "name",
            "features",
            "description",
            "catalog",
            "action",
            "args",
            "expect",
            "expectError",
        ],
        "case",
    )?;
    if case["action"] != "handle_rpc" {
        return Err("unexpected action".into());
    }
    keys(
        &case["args"],
        &[
            "message",
            "functionMetadata",
            "userActivationPresent",
            "outboundCall",
            "inboundResponse",
            "catalogId",
            "catalogVersion",
            "secondOutboundCall",
            "afterCall",
            "outboundListener",
            "lateInboundResponse",
            "surfaceId",
            "dataModel",
            "uiBindingScenario",
            "uiActionFallbackScenario",
        ],
        "args",
    )?;
    if let Some(expected) = case.get("expect") {
        keys(
            expected,
            &[
                "response",
                "error",
                "correlatedCallId",
                "result",
                "inFlightError",
                "lateResponseIgnored",
                "initialValue",
                "outboundCall",
                "resolvedValue",
                "siblings",
                "surfaceError",
                "validationResult",
                "dataModel",
            ],
            "expect",
        )?;
    }
    let mut p = processor(case)?;
    if case["args"].get("message").is_some() {
        inbound(&mut p, case)
    } else if case["args"].get("outboundCall").is_some() {
        outbound(&mut p, case)
    } else {
        Err("scenario requires mounted component/action-result API".into())
    }
}

#[test]
fn official_rpc_corpus() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/rpc_functions.yaml")).unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/rpc-coverage.json")).unwrap();
    assert_eq!(cases.len(), 33, "official corpus count changed");
    let unsupported = manifest["unsupported"].as_array().unwrap();
    let mut actual_unsupported = Vec::new();
    let mut passed = 0;
    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        if case["args"].get("uiBindingScenario").is_some()
            || case["args"].get("uiActionFallbackScenario").is_some()
        {
            let reason = if case["args"].get("uiBindingScenario").is_some() {
                "mounted component resolver, structured rule results, and correlated surface errors are not exposed by the core host API"
            } else {
                "action completion values and correlated surface errors are not exposed by the core host API"
            };
            actual_unsupported.push(json!({"name":name,"reason":reason}));
            continue;
        }
        match run(&case) {
            Ok(()) => passed += 1,
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    for failure in &failures {
        eprintln!("DIVERGENCE {failure}");
    }
    for case in &actual_unsupported {
        eprintln!("UNSUPPORTED {case}");
    }
    eprintln!(
        "official RPC corpus: {passed} pass, {} diverged, {} unsupported",
        failures.len(),
        actual_unsupported.len()
    );
    assert_eq!(
        &actual_unsupported, unsupported,
        "coverage changes must be explicit"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(passed, manifest["passed"].as_u64().unwrap() as usize);
}
