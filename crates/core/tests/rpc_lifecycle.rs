use a2ui_base::{
    catalog::Catalog,
    message_processor::MessageProcessor,
    protocol::rpc::{CallAgentFunction, FunctionOutcome},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn send(p: &mut MessageProcessor, value: Value) {
    p.process_message(MessageProcessor::parse_message(&value.to_string()).unwrap())
        .unwrap();
}
fn processor() -> MessageProcessor {
    let mut p = MessageProcessor::new(vec![Catalog::new("test")]);
    for id in ["s", "other"] {
        send(
            &mut p,
            json!({"version":"1.0","createSurface":{"surfaceId":id,"catalogId":"test"}}),
        );
    }
    p
}
fn call(id: &str, surface: &str) -> CallAgentFunction {
    serde_json::from_value(json!({"functionCallId":id,"surfaceId":surface,"callFunction":{"@call":"remote","catalogId":"test","args":{}}})).unwrap()
}
fn error_code(result: &a2ui_base::protocol::rpc::FunctionResponse) -> &str {
    match &result.outcome {
        FunctionOutcome::Failure(f) => &f.error.code,
        _ => panic!("expected failure"),
    }
}
#[test]
fn explicit_rpc_correlates_once_and_ignores_unknown_or_duplicate_replies() {
    let mut p = processor();
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":"unknown","value":1}}),
    );
    assert!(p.drain_agent_function_responses().is_empty());
    assert!(p.drain_agent_call_results().is_empty());
    p.begin_agent_call(call("a", "s"), None, Instant::now())
        .unwrap();
    assert_eq!(p.drain_outgoing().len(), 1);
    let response =
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":"a","value":null}});
    send(&mut p, response.clone());
    assert_eq!(p.drain_agent_function_responses().len(), 1);
    assert_eq!(p.drain_agent_call_results().len(), 1);
    send(&mut p, response);
    assert!(p.drain_agent_function_responses().is_empty());
    assert!(p.drain_agent_call_results().is_empty());
}
#[test]
fn deadlines_are_deterministic_and_late_replies_cannot_resettle() {
    let mut p = processor();
    let now = Instant::now();
    p.begin_agent_call(call("deadline", "s"), Some(Duration::from_millis(10)), now)
        .unwrap();
    p.expire_agent_calls(now + Duration::from_millis(9));
    assert!(p.drain_agent_call_results().is_empty());
    p.expire_agent_calls(now + Duration::from_millis(10));
    assert_eq!(error_code(&p.drain_agent_call_results()[0]), "TIMEOUT");
    assert!(
        p.drain_outgoing().is_empty(),
        "undelivered expired call must be removed"
    );
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":"deadline","value":"late"}}),
    );
    assert!(p.drain_agent_function_responses().is_empty());
    assert!(p.drain_agent_call_results().is_empty());
}
#[test]
fn duplicate_pending_ids_do_not_replace_the_original() {
    let mut p = processor();
    let now = Instant::now();
    p.begin_agent_call(call("same", "s"), None, now).unwrap();
    assert_eq!(
        p.begin_agent_call(call("same", "other"), None, now)
            .unwrap_err()
            .code,
        "DUPLICATE"
    );
    p.cancel_surface_calls("other");
    assert!(p.drain_agent_call_results().is_empty());
    p.cancel_surface_calls("s");
    assert_eq!(error_code(&p.drain_agent_call_results()[0]), "CANCELLED");
}
#[test]
fn cancellation_and_deletion_are_surface_scoped() {
    let mut p = processor();
    let now = Instant::now();
    p.begin_agent_call(call("one", "s"), None, now).unwrap();
    p.begin_agent_call(call("two", "other"), None, now).unwrap();
    send(
        &mut p,
        json!({"version":"1.0","deleteSurface":{"surfaceId":"s"}}),
    );
    let result = p.drain_agent_call_results();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].function_call_id, "one");
    assert_eq!(error_code(&result[0]), "CANCELLED");
    assert_eq!(p.drain_outgoing().len(), 1);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":"two","value":2}}),
    );
    assert_eq!(p.drain_agent_call_results().len(), 1);
}
#[test]
fn disposed_rpc_state_cancels_pending_and_stays_closed_until_reset() {
    let mut p = processor();
    let now = Instant::now();
    p.begin_agent_call(call("one", "s"), None, now).unwrap();
    p.dispose_agent_calls();
    assert_eq!(error_code(&p.drain_agent_call_results()[0]), "CANCELLED");
    assert_eq!(
        p.begin_agent_call(call("two", "s"), None, now)
            .unwrap_err()
            .code,
        "DISPOSED"
    );
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":"one","value":2}}),
    );
    assert!(p.drain_agent_call_results().is_empty());
    p.reset();
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"s"}}),
    );
    p.begin_agent_call(call("new", "s"), None, Instant::now())
        .unwrap();
}
#[test]
fn absent_transport_or_surface_never_queues_rpc() {
    let mut p = processor();
    let now = Instant::now();
    p.set_agent_transport_available(false);
    assert_eq!(
        p.begin_agent_call(call("one", "s"), None, now)
            .unwrap_err()
            .code,
        "NO_LISTENER"
    );
    assert!(p.drain_outgoing().is_empty());
    p.set_agent_transport_available(true);
    assert_eq!(
        p.begin_agent_call(call("one", "missing"), None, now)
            .unwrap_err()
            .code,
        "SURFACE_NOT_FOUND"
    );
    assert!(p.drain_outgoing().is_empty());
}
#[test]
fn explicit_and_reactive_calls_keep_separate_lifetimes() {
    use a2ui_base::evaluation::{EvaluationMode, EvaluationStatus};
    let mut p = processor();
    let now = Instant::now();
    let id = p
        .start_evaluation(
            "s",
            "",
            json!({"@call":"reactive"}),
            EvaluationMode::Reactive,
        )
        .unwrap();
    p.drain_evaluation_updates();
    let reactive = serde_json::to_value(p.drain_outgoing().pop().unwrap()).unwrap();
    p.begin_agent_call(call("explicit", "s"), None, now)
        .unwrap();
    p.dispose_agent_calls();
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":reactive["callAgentFunction"]["functionCallId"],"value":42}}),
    );
    assert!(
        p.drain_evaluation_updates()
            .iter()
            .any(|u| u.id == id && u.status == EvaluationStatus::Ready(json!(42)))
    );
    assert!(p.drain_agent_function_responses().is_empty());
}

#[test]
fn failed_begin_still_prunes_other_calls_that_expired_during_begin() {
    let mut p = processor();
    let now = Instant::now();
    p.begin_agent_call(call("expires", "s"), Some(Duration::from_millis(1)), now)
        .unwrap();
    p.begin_agent_call(call("duplicate", "s"), None, now)
        .unwrap();
    assert_eq!(
        p.begin_agent_call(call("duplicate", "s"), None, now + Duration::from_millis(1))
            .unwrap_err()
            .code,
        "DUPLICATE"
    );
    let outgoing = p.drain_outgoing();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(
        serde_json::to_value(&outgoing[0]).unwrap()["callAgentFunction"]["functionCallId"],
        "duplicate"
    );
    assert_eq!(error_code(&p.drain_agent_call_results()[0]), "TIMEOUT");
}

#[test]
fn explicit_calls_cannot_alias_an_evaluator_owned_id() {
    use a2ui_base::evaluation::EvaluationMode;
    let mut p = processor();
    p.start_evaluation("s", "", json!({"@call":"remote"}), EvaluationMode::Reactive)
        .unwrap();
    let outgoing = serde_json::to_value(p.drain_outgoing().pop().unwrap()).unwrap();
    let id = outgoing["callAgentFunction"]["functionCallId"]
        .as_str()
        .unwrap();
    assert_eq!(
        p.begin_agent_call(call(id, "s"), None, Instant::now())
            .unwrap_err()
            .code,
        "DUPLICATE"
    );
    assert!(p.drain_outgoing().is_empty());
}
