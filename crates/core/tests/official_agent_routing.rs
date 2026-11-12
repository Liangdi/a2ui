//! Official resumable routing corpus audit. Every unsupported feature is reported.
use a2ui_base::{
    catalog::{
        Catalog, SchemaOnlyFunction, basic_functions::build_basic_functions,
        function_api::AllowedCallers,
    },
    evaluation::{EvaluationMode, EvaluationStatus},
    message_processor::MessageProcessor,
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn send(p: &mut MessageProcessor, v: Value) -> Result<(), String> {
    p.process_message(MessageProcessor::parse_message(&v.to_string()).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn require_subset(expected: &Value, actual: &Value) -> Result<(), String> {
    if let Some(object) = expected.as_object() {
        for (k, v) in object {
            if k == "message" {
                if !actual[k]
                    .as_str()
                    .unwrap_or("")
                    .contains(v.as_str().unwrap())
                {
                    return Err(format!(
                        "expected message containing {v}, got {}",
                        actual[k]
                    ));
                }
            } else {
                require_subset(v, &actual[k])?;
            }
        }
        Ok(())
    } else if expected == actual {
        Ok(())
    } else {
        Err(format!("expected {expected}, got {actual}"))
    }
}
fn run(case: &Value) -> Result<(), String> {
    let mut catalogs = Vec::new();
    for definition in case["catalogs"].as_array().unwrap() {
        let mut catalog = Catalog::new(definition["catalogId"].as_str().unwrap());
        for function in build_basic_functions() {
            if definition["localFunctions"]
                .as_array()
                .is_some_and(|names| names.iter().any(|name| name == function.name()))
            {
                catalog = catalog.with_function(function);
            }
        }
        if let Some(functions) = definition["agentFunctions"].as_object() {
            for (name, schema) in functions {
                let callers = match schema["allowedCallers"].as_str() {
                    Some("agentOnly") => AllowedCallers::AgentOnly,
                    Some("rendererOrAgent") => AllowedCallers::RendererOrAgent,
                    _ => AllowedCallers::RendererOnly,
                };
                let function = SchemaOnlyFunction::new(
                    name.clone(),
                    a2ui_base::catalog::schema_only::parse_return_type(
                        schema["returnType"].as_str().unwrap_or("any"),
                    ),
                )
                .with_policy(
                    schema.get("args").cloned().unwrap_or(json!({})),
                    callers,
                    schema["requiresUserActivation"].as_bool().unwrap_or(false),
                )
                .map_err(|e| e.to_string())?;
                catalog = catalog.with_function(Box::new(function));
            }
        }
        catalogs.push(catalog);
    }
    let mut p = MessageProcessor::new(catalogs);
    if let Some(available) = case["surface"]["outboundListener"].as_bool() {
        p.set_agent_transport_available(available);
    }
    let mut surface =
        json!({"surfaceId":"main","dataModel":case.get("dataModel").cloned().unwrap_or(json!({}))});
    if let Some(id) = case["surface"].get("catalogId") {
        surface["catalogId"] = id.clone();
    }
    send(&mut p, json!({"version":"1.0","createSurface":surface}))?;
    let mut ids = Vec::new();
    let mut check_ids = std::collections::HashSet::new();
    let mut values: HashMap<String, Value> = HashMap::new();
    let mut calls = Vec::new();
    for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let outcome = (|| -> Result<(), String> {
            for key in step.as_object().unwrap().keys() {
                if ![
                    "evaluate",
                    "as",
                    "respond",
                    "updateData",
                    "deleteSurface",
                    "expectValues",
                    "expectErrors",
                    "expectOutbound",
                    "expectActions",
                    "outboundInAnyOrder",
                    "cancelSurfaceCalls",
                ]
                .contains(&key.as_str())
                {
                    return Err(format!("unknown step key {key}"));
                }
            }
            if let Some(expression) = step.get("evaluate") {
                match step["as"].as_str() {
                    Some("event") => p
                        .start_current_action("main", "source", "", json!({"event":expression}))
                        .map_err(|e| e.to_string())?,
                    Some("action") => p
                        .start_current_action(
                            "main",
                            "source",
                            "",
                            json!({"functionCall":expression}),
                        )
                        .map_err(|e| e.to_string())?,
                    Some("check") => {
                        let id = p
                            .start_check("main", "", expression.clone())
                            .map_err(|e| e.to_string())?;
                        check_ids.insert(id.clone());
                        ids.push(id);
                    }
                    None => {
                        let id = p
                            .start_evaluation(
                                "main",
                                "",
                                expression.clone(),
                                EvaluationMode::Reactive,
                            )
                            .map_err(|e| e.to_string())?;
                        values.insert(id.clone(), Value::Null);
                        ids.push(id);
                    }
                    other => return Err(format!("unsupported evaluation mode: {other:?}")),
                }
            } else if let Some(response) = step.get("respond") {
                let call: &Value = calls
                    .get(response["call"].as_u64().unwrap() as usize)
                    .ok_or("response refers to missing call")?;
                let mut reply = json!({"functionCallId":call["functionCallId"]});
                if let Some(error) = response.get("error") {
                    reply["error"] = error.clone();
                } else {
                    reply["value"] = response["value"].clone();
                }
                send(
                    &mut p,
                    json!({"version":"1.0","agentFunctionResponse":reply}),
                )?;
            } else if let Some(update) = step.get("updateData") {
                send(
                    &mut p,
                    json!({"version":"1.0","updateDataModel":{"surfaceId":"main","path":update["path"],"value":update["value"]}}),
                )?;
            } else if step["cancelSurfaceCalls"] == true {
                p.cancel_surface_calls("main");
            } else if step["deleteSurface"] == true {
                send(
                    &mut p,
                    json!({"version":"1.0","deleteSurface":{"surfaceId":"main"}}),
                )?;
            } else {
                return Err("step has no supported operation".into());
            }
            let mut errors = Vec::new();
            for update in p.drain_evaluation_updates() {
                if check_ids.contains(&update.id) {
                    continue;
                }
                match update.status {
                    EvaluationStatus::Ready(value) => {
                        values.insert(update.id, value);
                    }
                    EvaluationStatus::Cancelled => {
                        values.insert(update.id, Value::Null);
                    }
                    EvaluationStatus::Pending => {
                        values.insert(update.id, Value::Null);
                    }
                    EvaluationStatus::Error { code, message } => {
                        values.insert(update.id, Value::Null);
                        errors.push(json!({"code":code,"message":message}));
                    }
                }
            }
            for diagnostic in p.drain_evaluation_diagnostics() {
                errors.push(json!({"code":diagnostic.code,"message":diagnostic.message}));
            }
            for update in p.drain_check_updates() {
                values.insert(update.id, json!(update.valid));
                if let Some(error) = update.error {
                    errors.push(json!({"code":error.code,"message":error.message}));
                }
            }
            let action_errors: Vec<Value> = p.drain_action_errors().into_iter().map(|error|
                json!({"code":error.code,"message":error.message,"surfaceId":error.surface_id})
            ).collect();
            errors.extend(action_errors.iter().cloned());
            let mut wire_errors = Vec::new();
            let mut outbound = Vec::new();
            let mut actions = Vec::new();
            for message in p.drain_outgoing() {
                let message = serde_json::to_value(message).unwrap();
                if let Some(call) = message.get("callAgentFunction") {
                    outbound.push(call.clone());
                } else if let Some(action) = message.get("action") {
                    actions.push(action.clone());
                } else if let Some(error) = message.get("error") {
                    wire_errors.push(error.clone());
                } else {
                    return Err(format!("unexpected outgoing message: {message}"));
                }
            }
            if wire_errors.len() != action_errors.len() {
                return Err("action error channel and wire error count differ".into());
            }
            for (expected, actual) in action_errors.iter().zip(&wire_errors) {
                require_subset(expected, actual)?;
            }
            let expected_actions = step
                .get("expectActions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if expected_actions.len() != actions.len() {
                return Err(format!(
                    "actions expected {expected_actions:?}, got {actions:?}"
                ));
            }
            for (expected, actual) in expected_actions.iter().zip(&actions) {
                require_subset(expected, actual)?;
            }
            let mut actual: Vec<Value> = outbound
                .iter()
                .map(|call| {
                    let mut function = call["callFunction"].clone();
                    if function.get("args").is_none() {
                        function["args"] = json!({});
                    }
                    function
                })
                .collect();
            let mut expected = step
                .get("expectOutbound")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if step["outboundInAnyOrder"] == true {
                actual.sort_by_key(Value::to_string);
                expected.sort_by_key(Value::to_string);
            }
            if expected != actual {
                return Err(format!("outbound expected {expected:?}, got {actual:?}"));
            }
            calls.extend(outbound);
            if let Some(expected) = step.get("expectValues") {
                let actual = Value::Array(
                    ids.iter()
                        .map(|id| values.get(id).cloned().unwrap_or(Value::Null))
                        .collect(),
                );
                if *expected != actual {
                    return Err(format!("values expected {expected}, got {actual}"));
                }
            }
            let expected = step
                .get("expectErrors")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if expected.len() != errors.len() {
                return Err(format!("errors expected {expected:?}, got {errors:?}"));
            }
            for (e, a) in expected.iter().zip(&errors) {
                require_subset(e, a)?;
            }
            Ok(())
        })();
        outcome.map_err(|e| format!("step {index}: {e}"))?;
    }
    Ok(())
}

#[test]
fn official_agent_routing_corpus() {
    let cases: Vec<serde_yaml_ng::Value> =
        serde_yaml_ng::from_str(include_str!("conformance/agent_function_routing.yaml")).unwrap();
    let mut passed = 0;
    let mut failures = Vec::new();
    let mut unsupported = Vec::new();
    for raw in cases {
        let name = raw["name"].as_str().unwrap().to_owned();
        if name == "test_agent_routing_non_json_argument_fails_locally" {
            unsupported.push((
                name,
                "JSON-only public API cannot represent nonfinite values",
            ));
            continue;
        }
        let case: Value = serde_json::to_value(raw).unwrap();
        assert_eq!(case["action"], "route_agent_function");
        let reason = if case["protocolVersion"] != "1.0" {
            Some("legacy v0.9 surface API not implemented")
        } else {
            None
        };
        if let Some(reason) = reason {
            unsupported.push((name, reason));
            continue;
        }
        match run(&case) {
            Ok(()) => passed += 1,
            Err(error) => failures.push(json!({"name":name,"error":error})),
        }
    }
    eprintln!(
        "official routing: {passed} pass, {} diverged, {} unsupported",
        failures.len(),
        unsupported.len()
    );
    for failure in &failures {
        eprintln!("DIVERGENCE {failure}");
    }
    for omitted in &unsupported {
        eprintln!("UNSUPPORTED {}: {}", omitted.0, omitted.1);
    }
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/agent-routing-coverage.json")).unwrap();
    let actual: Vec<Value> = unsupported
        .iter()
        .map(|(name, reason)| json!({"name":name,"reason":reason}))
        .collect();
    assert_eq!(
        json!(actual),
        manifest["unsupported"],
        "unsupported coverage changed; update manifest after implementing cases"
    );
    assert!(failures.is_empty());
}
