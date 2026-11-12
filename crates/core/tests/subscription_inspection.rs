use a2ui_base::{
    evaluation::{EvaluationMode, EvaluationStatus},
    message_processor::MessageProcessor,
};
use serde_json::json;

#[test]
fn reading_subscription_state_does_not_consume_host_updates() {
    let mut p = MessageProcessor::new(vec![]);
    p.process_message(
        MessageProcessor::parse_message(r#"{"version":"1.0","createSurface":{"surfaceId":"s"}}"#)
            .unwrap(),
    )
    .unwrap();
    let id = p
        .start_evaluation("s", "", json!("ready"), EvaluationMode::Reactive)
        .unwrap();
    assert_eq!(
        p.evaluation_status(&id),
        Some(EvaluationStatus::Ready(json!("ready")))
    );
    assert!(!p.drain_evaluation_updates().is_empty());
    let check = p.start_check("s", "", json!({"valid":false})).unwrap();
    assert!(!p.check_status(&check).unwrap().valid);
    assert!(!p.drain_check_updates().is_empty());
}
