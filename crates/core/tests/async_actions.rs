use a2ui_base::catalog::Catalog;
use a2ui_base::message_processor::MessageProcessor;
use serde_json::{Value, json};

fn processor() -> MessageProcessor {
    let mut processor = MessageProcessor::new(vec![Catalog::new("test")]);
    send(
        &mut processor,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"test"}}),
    );
    processor
}
fn send(p: &mut MessageProcessor, value: Value) {
    p.process_message(MessageProcessor::parse_message(&value.to_string()).unwrap())
        .unwrap();
}
fn output(p: &mut MessageProcessor) -> Vec<Value> {
    p.drain_outgoing()
        .into_iter()
        .map(|m| serde_json::to_value(m).unwrap())
        .collect()
}
fn answer(p: &mut MessageProcessor, message: &Value, value: Value) {
    send(
        p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":message["callAgentFunction"]["functionCallId"],"value":value}}),
    );
}

#[test]
fn event_context_is_sequential_and_dispatches_actual_action() {
    let mut p = processor();
    let action: Value = serde_json::from_str(r#"{"event":{"name":"buy","context":{"zFirst":{"@call":"first"},"aSecond":{"@call":"second"}},"userMessage":"Bought"}}"#).unwrap();
    p.start_current_action("s", "button", "", action).unwrap();
    let first = output(&mut p);
    assert_eq!(first.len(), 1);
    assert_eq!(
        first[0]["callAgentFunction"]["callFunction"]["@call"],
        "first"
    );
    answer(&mut p, &first[0], json!(1));
    let second = output(&mut p);
    assert_eq!(second.len(), 1);
    assert_eq!(
        second[0]["callAgentFunction"]["callFunction"]["@call"],
        "second"
    );
    answer(&mut p, &second[0], json!(2));
    let action = output(&mut p);
    assert_eq!(action.len(), 1);
    assert_eq!(
        action[0]["action"]["context"],
        json!({"zFirst":1,"aSecond":2})
    );
    assert_eq!(action[0]["action"]["userMessage"], "Bought");
    assert!(p.drain_evaluation_updates().is_empty());
}

#[test]
fn failed_first_context_stops_later_calls_and_reports_once() {
    let mut p = processor();
    p.start_current_action("s","button","",json!({"event":{"name":"buy","context":{"first":{"@call":"remote"},"second":{"@call":"never"}}}})).unwrap();
    let call = output(&mut p).remove(0);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["callAgentFunction"]["functionCallId"],"error":{"code":"DENIED","message":"No"}}}),
    );
    let errors = p.drain_action_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "EXPRESSION_ERROR");
    assert!(errors[0].message.contains("first"));
    let messages = output(&mut p);
    assert_eq!(messages.len(), 1);
    assert!(messages[0].get("error").is_some());
}

#[test]
fn user_message_null_is_absent_and_nonstring_is_invalid() {
    let mut p = processor();
    p.start_current_action(
        "s",
        "button",
        "",
        json!({"event":{"name":"send","userMessage":{"@path":"/missing"}}}),
    )
    .unwrap();
    let sent = output(&mut p);
    assert!(sent[0]["action"].get("userMessage").is_none());
    p.start_current_action(
        "s",
        "button",
        "",
        json!({"event":{"name":"send","userMessage":3}}),
    )
    .unwrap();
    assert_eq!(p.drain_action_errors()[0].code, "INVALID_ACTION");
    assert!(output(&mut p)[0].get("error").is_some());
}

#[test]
fn checks_are_nonfailing_pending_and_fail_closed_on_error() {
    let mut p = processor();
    let id = p.start_check("s", "", json!({"@call":"check"})).unwrap();
    let pending = p.drain_check_updates();
    assert_eq!(pending[0].id, id);
    assert!(pending[0].valid && pending[0].pending);
    let call = output(&mut p).remove(0);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["callAgentFunction"]["functionCallId"],"error":{"code":"NO","message":"unavailable"}}}),
    );
    let failed = p.drain_check_updates();
    assert!(!failed[0].valid && !failed[0].pending);
    assert_eq!(failed[0].error.as_ref().unwrap().code, "EXPRESSION_ERROR");
}

#[test]
fn checks_accept_structured_validation_results_and_cancel_cleanly() {
    let mut p = processor();
    let id = p.start_check("s", "", json!({"@call":"check"})).unwrap();
    let call = output(&mut p).remove(0);
    p.drain_check_updates();
    answer(&mut p, &call, json!({"valid":false,"message":"bad"}));
    assert!(!p.drain_check_updates()[0].valid);
    p.cancel_check(&id);
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/other","value":1}}),
    );
    assert!(p.drain_check_updates().is_empty());
}

#[test]
fn deleting_surface_silently_discards_pending_event() {
    let mut p = processor();
    p.start_current_action(
        "s",
        "button",
        "",
        json!({"event":{"name":"buy","context":{"a":{"@call":"remote"}}}}),
    )
    .unwrap();
    let call = output(&mut p).remove(0);
    send(
        &mut p,
        json!({"version":"1.0","deleteSurface":{"surfaceId":"s"}}),
    );
    answer(&mut p, &call, json!(true));
    assert!(output(&mut p).is_empty());
    assert!(p.drain_action_errors().is_empty());
    assert!(p.drain_evaluation_updates().is_empty());
    p.start_current_action("s", "button", "", json!({"event":{"name":"buy"}}))
        .unwrap();
    assert!(output(&mut p).is_empty());
}

#[test]
fn cancelled_and_deleted_checks_release_queued_updates_and_internal_handles() {
    let mut p = processor();
    for _ in 0..20 {
        let id = p.start_check("s", "", json!({"@call":"check"})).unwrap();
        p.cancel_check(&id);
    }
    assert!(p.drain_check_updates().is_empty());
    assert!(p.drain_evaluation_updates().is_empty());
    assert!(output(&mut p).is_empty());
    p.start_check("s", "", json!({"@call":"check"})).unwrap();
    send(
        &mut p,
        json!({"version":"1.0","deleteSurface":{"surfaceId":"s"}}),
    );
    assert!(p.drain_check_updates().is_empty());
    assert!(p.drain_evaluation_updates().is_empty());
    assert!(output(&mut p).is_empty());
}
