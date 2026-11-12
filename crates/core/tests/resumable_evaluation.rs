use a2ui_base::catalog::Catalog;
use a2ui_base::evaluation::{EvaluationMode, EvaluationStatus};
use a2ui_base::message_processor::MessageProcessor;
use serde_json::{Value, json};

fn processor() -> MessageProcessor {
    let mut p = MessageProcessor::new(vec![Catalog::new("test")]);
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"test","dataModel":{"name":"first"}}}),
    );
    p
}
fn send(p: &mut MessageProcessor, message: Value) {
    p.process_message(MessageProcessor::parse_message(&message.to_string()).unwrap())
        .unwrap();
}
fn calls(p: &mut MessageProcessor) -> Vec<Value> {
    p.drain_outgoing()
        .into_iter()
        .map(|m| serde_json::to_value(m).unwrap()["callAgentFunction"].clone())
        .collect()
}
fn answer(p: &mut MessageProcessor, call: &Value, value: Value) {
    send(
        p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"value":value}}),
    );
}
fn state(p: &mut MessageProcessor, id: &str) -> EvaluationStatus {
    p.drain_evaluation_updates()
        .into_iter()
        .rfind(|u| u.id == id)
        .unwrap()
        .status
}

#[test]
fn sibling_remote_arguments_run_in_parallel_then_parent_resumes() {
    let mut p = processor();
    let id = p
        .start_evaluation(
            "s",
            "",
            json!({"@call":"combine","args":{
                "left":{"@call":"left"}, "right":{"@call":"right"}
            }}),
            EvaluationMode::Reactive,
        )
        .unwrap();
    assert_eq!(state(&mut p, &id), EvaluationStatus::Pending);
    let first = calls(&mut p);
    assert_eq!(first.len(), 2);
    answer(&mut p, &first[1], json!(2));
    assert!(calls(&mut p).is_empty());
    answer(&mut p, &first[0], json!(1));
    let parent = calls(&mut p);
    assert_eq!(parent.len(), 1);
    assert_eq!(parent[0]["callFunction"]["@call"], "combine");
    assert_eq!(
        parent[0]["callFunction"]["args"].as_object().unwrap().len(),
        2
    );
    answer(&mut p, &parent[0], json!({"ok":true}));
    assert_eq!(
        state(&mut p, &id),
        EvaluationStatus::Ready(json!({"ok":true}))
    );
}

#[test]
fn reactive_calls_share_but_actions_are_fresh() {
    let mut p = processor();
    let expression = json!({"@call":"remote","args":{"x":1}});
    let a = p
        .start_evaluation("s", "", expression.clone(), EvaluationMode::Reactive)
        .unwrap();
    let b = p
        .start_evaluation("s", "", expression.clone(), EvaluationMode::Reactive)
        .unwrap();
    let shared = calls(&mut p);
    assert_eq!(shared.len(), 1);
    p.drain_evaluation_updates();
    answer(&mut p, &shared[0], json!(3));
    let updates = p.drain_evaluation_updates();
    assert!(
        updates
            .iter()
            .any(|u| u.id == a && u.status == EvaluationStatus::Ready(json!(3)))
    );
    assert!(
        updates
            .iter()
            .any(|u| u.id == b && u.status == EvaluationStatus::Ready(json!(3)))
    );
    p.start_evaluation("s", "", expression.clone(), EvaluationMode::Action)
        .unwrap();
    p.start_evaluation("s", "", expression, EvaluationMode::Action)
        .unwrap();
    let separate = calls(&mut p);
    assert_eq!(separate.len(), 2);
    assert_ne!(separate[0]["functionCallId"], separate[1]["functionCallId"]);
}

#[test]
fn data_update_invalidates_cache_and_ignores_late_generation() {
    let mut p = processor();
    let id = p
        .start_evaluation(
            "s",
            "",
            json!({"@call":"remote","args":{"name":{"@path":"/name"}}}),
            EvaluationMode::Reactive,
        )
        .unwrap();
    let old = calls(&mut p).remove(0);
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/name","value":"second"}}),
    );
    let new = calls(&mut p).remove(0);
    assert_eq!(new["callFunction"]["args"]["name"], "second");
    p.drain_evaluation_updates();
    answer(&mut p, &old, json!("stale"));
    assert!(p.drain_evaluation_updates().is_empty());
    answer(&mut p, &new, json!("fresh"));
    assert_eq!(state(&mut p, &id), EvaluationStatus::Ready(json!("fresh")));
}

#[test]
fn surface_deletion_cancels_pending_work_and_unsent_calls() {
    let mut p = processor();
    let id = p
        .start_evaluation("s", "", json!({"@call":"remote"}), EvaluationMode::Reactive)
        .unwrap();
    send(
        &mut p,
        json!({"version":"1.0","deleteSurface":{"surfaceId":"s"}}),
    );
    assert!(calls(&mut p).is_empty());
    assert!(matches!(state(&mut p, &id), EvaluationStatus::Cancelled));
}

#[test]
fn cancelling_one_subscriber_does_not_cancel_shared_call() {
    let mut p = processor();
    let expression = json!({"@call":"remote"});
    let a = p
        .start_evaluation("s", "", expression.clone(), EvaluationMode::Reactive)
        .unwrap();
    let b = p
        .start_evaluation("s", "", expression, EvaluationMode::Reactive)
        .unwrap();
    p.cancel_evaluation(&a);
    let call = calls(&mut p).remove(0);
    p.drain_evaluation_updates();
    answer(&mut p, &call, json!(true));
    assert_eq!(state(&mut p, &b), EvaluationStatus::Ready(json!(true)));
}

#[test]
fn escaped_objects_and_numbers_are_not_reinterpreted() {
    let mut p = processor();
    let id = p
        .start_evaluation(
            "s",
            "",
            json!({"@@call":"literal","n":9007199254740993u64}),
            EvaluationMode::Reactive,
        )
        .unwrap();
    assert_eq!(
        state(&mut p, &id),
        EvaluationStatus::Ready(json!({"@call":"literal","n":9007199254740993u64}))
    );
    assert!(calls(&mut p).is_empty());
}

#[test]
fn remote_errors_are_not_successful_null_results() {
    let mut p = processor();
    let id = p
        .start_evaluation("s", "", json!({"@call":"remote"}), EvaluationMode::Reactive)
        .unwrap();
    let call = calls(&mut p).remove(0);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"error":{"code":"DENIED","message":"No"}}}),
    );
    assert_eq!(
        state(&mut p, &id),
        EvaluationStatus::Error {
            code: "EXPRESSION_ERROR".into(),
            message: "No".into()
        }
    );
}

#[test]
fn data_updates_cancel_pending_actions_instead_of_mixing_old_and_new_arguments() {
    let mut p = processor();
    let id = p
        .start_evaluation("s", "", json!({"@call":"remote"}), EvaluationMode::Action)
        .unwrap();
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/name","value":"second"}}),
    );
    assert!(calls(&mut p).is_empty());
    assert!(matches!(state(&mut p, &id), EvaluationStatus::Cancelled));
}

#[test]
fn native_action_is_not_repeated_when_remote_sibling_resumes() {
    use a2ui_base::catalog::function_api::{FunctionImplementation, ReturnType};
    use a2ui_base::model::data_context::DataContext;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Counter(Arc<AtomicUsize>);
    impl FunctionImplementation for Counter {
        fn name(&self) -> &'static str {
            "count"
        }
        fn return_type(&self) -> ReturnType {
            ReturnType::Number
        }
        fn execute(
            &self,
            _: &std::collections::HashMap<String, Value>,
            _: &DataContext,
        ) -> a2ui_base::error::Result<Value> {
            Ok(json!(self.0.fetch_add(1, Ordering::SeqCst)))
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let mut p = MessageProcessor::new(vec![
        Catalog::new("test").with_function(Box::new(Counter(count.clone()))),
    ]);
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"test"}}),
    );
    let id = p
        .start_evaluation(
            "s",
            "",
            json!([{"@call":"count"},{"@call":"remote"}]),
            EvaluationMode::Action,
        )
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let call = calls(&mut p).remove(0);
    answer(&mut p, &call, json!(2));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(state(&mut p, &id), EvaluationStatus::Ready(json!([0, 2])));
}

#[test]
fn identical_calls_on_different_surfaces_are_not_shared() {
    let mut p = processor();
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"other","catalogId":"test"}}),
    );
    for surface in ["s", "other"] {
        p.start_evaluation(
            surface,
            "",
            json!({"@call":"remote"}),
            EvaluationMode::Reactive,
        )
        .unwrap();
    }
    assert_eq!(calls(&mut p).len(), 2);
}

#[test]
fn failed_binding_retries_only_when_args_change_but_new_binding_gets_fresh_call() {
    let mut p = processor();
    let expression = json!({"@call":"remote","args":{"name":{"@path":"/name"}}});
    p.start_evaluation("s", "", expression.clone(), EvaluationMode::Reactive)
        .unwrap();
    let call = calls(&mut p).remove(0);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"error":{"code":"NO","message":"failed"}}}),
    );
    let errors = p.drain_outgoing();
    assert_eq!(errors.len(), 1);
    let error = serde_json::to_value(&errors[0]).unwrap();
    assert_eq!(error["error"]["functionCallId"], call["functionCallId"]);
    assert_eq!(error["error"]["code"], "EXPRESSION_ERROR");
    assert!(error["error"].get("surfaceId").is_none());
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/unrelated","value":true}}),
    );
    assert!(calls(&mut p).is_empty());
    p.start_evaluation("s", "", expression, EvaluationMode::Reactive)
        .unwrap();
    assert_eq!(calls(&mut p).len(), 1);
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/name","value":"changed"}}),
    );
    assert_eq!(calls(&mut p).len(), 1);
}

#[test]
fn surface_call_cancellation_keeps_subscriptions_for_next_data_change() {
    let mut p = processor();
    let id = p
        .start_evaluation("s", "", json!({"@call":"remote"}), EvaluationMode::Reactive)
        .unwrap();
    let old = calls(&mut p).remove(0);
    p.cancel_surface_calls("s");
    p.drain_evaluation_updates();
    answer(&mut p, &old, json!("stale"));
    assert!(p.drain_evaluation_updates().is_empty());
    assert!(calls(&mut p).is_empty());
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/unrelated","value":true}}),
    );
    let new = calls(&mut p).remove(0);
    answer(&mut p, &new, json!("new"));
    assert_eq!(state(&mut p, &id), EvaluationStatus::Ready(json!("new")));
}

#[test]
fn unavailable_transport_fails_locally_without_queueing_an_rpc() {
    let mut p = processor();
    p.set_agent_transport_available(false);
    let id = p
        .start_evaluation("s", "", json!({"@call":"remote"}), EvaluationMode::Reactive)
        .unwrap();
    assert!(
        matches!(state(&mut p,&id),EvaluationStatus::Error{code,..} if code == "EXPRESSION_ERROR")
    );
    assert!(calls(&mut p).is_empty());
    p.cancel_evaluation(&id);
    p.set_agent_transport_available(true);
    p.start_evaluation("s", "", json!({"@call":"remote"}), EvaluationMode::Reactive)
        .unwrap();
    assert_eq!(calls(&mut p).len(), 1);
}
