use a2ui_base::{
    catalog::{Catalog, basic_functions::FormatStringFunction},
    message_processor::MessageProcessor,
};
use serde_json::{Value, json};
fn send(p: &mut MessageProcessor, message: Value) {
    p.process_message(MessageProcessor::parse_message(&message.to_string()).unwrap())
        .unwrap();
}
fn processor() -> MessageProcessor {
    let mut p = MessageProcessor::new(vec![
        Catalog::new("test").with_function(Box::new(FormatStringFunction)),
    ]);
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"test"}}),
    );
    p
}
fn calls(p: &mut MessageProcessor) -> Vec<Value> {
    p.drain_outgoing()
        .into_iter()
        .map(|m| serde_json::to_value(m).unwrap()["callAgentFunction"].clone())
        .collect()
}
fn respond(p: &mut MessageProcessor, call: &Value, value: Value) {
    send(
        p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"value":value}}),
    );
}
#[test]
fn local_function_result_is_available_once_under_host_action_handle() {
    let mut p = processor();
    let id = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"functionCall":{"@call":"formatString","args":{"value":"done"}}}),
        )
        .unwrap();
    assert_eq!(
        p.take_current_action_result(&id).unwrap().unwrap(),
        json!("done")
    );
    assert!(p.take_current_action_result(&id).is_none());
    assert!(p.drain_outgoing().is_empty());
}
#[test]
fn remote_function_results_correlate_out_of_order_without_reexecution() {
    let mut p = processor();
    let first = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"functionCall":{"@call":"first"}}),
        )
        .unwrap();
    let second = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"functionCall":{"@call":"second"}}),
        )
        .unwrap();
    let sent = calls(&mut p);
    assert_eq!(sent.len(), 2);
    assert!(p.take_current_action_result(&first).is_none());
    respond(&mut p, &sent[1], json!({"accepted":true}));
    assert_eq!(
        p.take_current_action_result(&second).unwrap().unwrap(),
        json!({"accepted":true})
    );
    assert!(p.take_current_action_result(&first).is_none());
    respond(&mut p, &sent[0], Value::Null);
    assert_eq!(
        p.take_current_action_result(&first).unwrap().unwrap(),
        Value::Null
    );
    assert!(p.drain_outgoing().is_empty());
}
#[test]
fn remote_failure_completes_error_without_losing_existing_error_channel() {
    let mut p = processor();
    let id = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"functionCall":{"@call":"remote"}}),
        )
        .unwrap();
    let sent = calls(&mut p);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":sent[0]["functionCallId"],"error":{"code":"EXECUTION_ERROR","message":"denied"}}}),
    );
    let error = p.take_current_action_result(&id).unwrap().unwrap_err();
    assert_eq!(error.code, "EXPRESSION_ERROR");
    assert!(error.message.contains("denied"));
    assert_eq!(p.drain_action_errors().len(), 1);
    let outgoing = p.drain_outgoing();
    assert_eq!(outgoing.len(), 1, "remote errors must not be emitted twice");
    assert_eq!(
        serde_json::to_value(&outgoing[0]).unwrap()["error"]["functionCallId"],
        sent[0]["functionCallId"]
    );
    assert!(p.take_current_action_result(&id).is_none());
}
#[test]
fn event_completes_null_only_after_real_dispatch() {
    let mut p = processor();
    let id = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"event":{"name":"done","context":{"answer":{"@call":"remote"}}}}),
        )
        .unwrap();
    let sent = calls(&mut p);
    assert!(p.take_current_action_result(&id).is_none());
    respond(&mut p, &sent[0], json!(42));
    assert_eq!(
        p.take_current_action_result(&id).unwrap().unwrap(),
        Value::Null
    );
    let outgoing = p.drain_outgoing();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(
        serde_json::to_value(&outgoing[0]).unwrap()["action"]["context"]["answer"],
        42
    );
}
#[test]
fn deleting_surface_cancels_completion_without_reporting_expression_error() {
    let mut p = processor();
    let id = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"functionCall":{"@call":"remote"}}),
        )
        .unwrap();
    let sent = calls(&mut p);
    send(
        &mut p,
        json!({"version":"1.0","deleteSurface":{"surfaceId":"s"}}),
    );
    assert_eq!(
        p.take_current_action_result(&id).unwrap().unwrap_err().code,
        "CANCELLED"
    );
    assert!(p.drain_action_errors().is_empty());
    respond(&mut p, &sent[0], json!("late"));
    assert!(p.take_current_action_result(&id).is_none());
    assert!(p.drain_outgoing().is_empty());
}
#[test]
fn retained_completion_capacity_is_bounded_and_taking_results_releases_slots() {
    let mut p = processor();
    let action = json!({"functionCall":{"@call":"formatString","args":{"value":"x"}}});
    let mut ids = Vec::new();
    for _ in 0..1024 {
        ids.push(
            p.start_current_action_with_result("s", "button", "", action.clone())
                .unwrap(),
        );
    }
    assert!(
        p.start_current_action_with_result("s", "button", "", action.clone())
            .is_err()
    );
    assert_eq!(
        p.take_current_action_result(&ids[0]).unwrap().unwrap(),
        json!("x")
    );
    assert!(
        p.start_current_action_with_result("s", "button", "", action.clone())
            .is_ok()
    );
    // Untracked callers do not acquire a requirement to drain the new result queue.
    p.start_current_action("s", "button", "", action).unwrap();
    assert!(p.drain_outgoing().is_empty());
}
#[test]
fn malformed_or_missing_surface_actions_do_not_allocate_result_handles() {
    let mut p = processor();
    assert!(
        p.start_current_action_with_result("missing", "button", "", json!({"event":{"name":"x"}}))
            .is_err()
    );
    assert!(
        p.start_current_action_with_result("s", "button", "", json!({}))
            .is_err()
    );
    assert!(p.drain_outgoing().is_empty());
}

#[test]
fn cancelling_surface_calls_settles_action_handle_without_next_response() {
    let mut p = processor();
    let id = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"functionCall":{"@call":"remote"}}),
        )
        .unwrap();
    let sent = calls(&mut p);
    p.cancel_surface_calls("s");
    assert_eq!(
        p.take_current_action_result(&id).unwrap().unwrap_err().code,
        "CANCELLED"
    );
    assert!(p.drain_action_errors().is_empty());
    respond(&mut p, &sent[0], json!("late"));
    assert!(p.take_current_action_result(&id).is_none());
}

#[test]
fn event_failure_keeps_call_correlation_and_context_in_single_wire_error() {
    let mut p = processor();
    let id = p
        .start_current_action_with_result(
            "s",
            "button",
            "",
            json!({"event":{"name":"done","context":{"rate":{"@call":"remote"}}}}),
        )
        .unwrap();
    let sent = calls(&mut p);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":sent[0]["functionCallId"],"error":{"code":"EXECUTION_ERROR","message":"Rate unavailable"}}}),
    );
    let error = p.take_current_action_result(&id).unwrap().unwrap_err();
    assert!(error.message.contains("context entry 'rate'"));
    let outgoing = p.drain_outgoing();
    assert_eq!(outgoing.len(), 1);
    let wire = serde_json::to_value(&outgoing[0]).unwrap();
    assert_eq!(wire["error"]["message"], error.message);
    assert_eq!(wire["error"]["functionCallId"], sent[0]["functionCallId"]);
}
