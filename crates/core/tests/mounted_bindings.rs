use a2ui_base::{catalog::Catalog, message_processor::MessageProcessor};
use serde_json::{Value, json};
fn send(p: &mut MessageProcessor, v: Value) {
    p.process_json_message(&v.to_string()).unwrap();
}
fn setup() -> MessageProcessor {
    let mut p = MessageProcessor::new(vec![Catalog::new("c")]);
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"c","dataModel":{"name":"Ada"},"components":[{"id":"root","component":"Text","text":{"@path":"/name"}},{"id":"sibling","component":"Text","text":"static"}]}}),
    );
    p
}
#[test]
fn mounted_properties_follow_data_and_component_replacement_and_delete() {
    let mut p = setup();
    let id = p.mount_property("s", "root", "text", "").unwrap();
    let sibling = p.mount_property("s", "sibling", "text", "").unwrap();
    assert_eq!(p.mounted_value(&id), Some(json!("Ada")));
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/name","value":"Grace"}}),
    );
    assert_eq!(p.mounted_value(&id), Some(json!("Grace")));
    send(
        &mut p,
        json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Text","text":"replacement"}]}}),
    );
    assert_eq!(p.mounted_value(&id), Some(json!("replacement")));
    assert_eq!(p.mounted_value(&sibling), Some(json!("static")));
    send(
        &mut p,
        json!({"version":"1.0","deleteSurface":{"surfaceId":"s"}}),
    );
    assert_eq!(p.mounted_value(&id), None);
    assert_eq!(p.mounted_value(&sibling), None);
}
#[test]
fn failed_remote_binding_emits_one_correlated_error_and_unmount_rejects_late_reply() {
    let mut p = setup();
    send(
        &mut p,
        json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Text","text":{"@call":"remote","args":{}}}]}}),
    );
    let id = p.mount_property("s", "root", "text", "").unwrap();
    assert_eq!(p.mounted_value(&id), Some(Value::Null));
    let call = serde_json::to_value(p.drain_outgoing().pop().unwrap()).unwrap();
    let call_id = call["callAgentFunction"]["functionCallId"].clone();
    let response = json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call_id,"error":{"code":"REMOTE","message":"failed"}}});
    send(&mut p, response.clone());
    let messages = p.drain_outgoing();
    assert_eq!(messages.len(), 1);
    let error = serde_json::to_value(&messages[0]).unwrap();
    assert_eq!(error["error"]["functionCallId"], call_id);
    assert_eq!(error["error"]["surfaceId"], "s");
    assert_eq!(p.mounted_value(&id), Some(Value::Null));
    send(&mut p, response);
    assert!(p.drain_outgoing().is_empty());
    p.unmount_property(&id);
    assert_eq!(p.mounted_value(&id), None);
    let id = p.mount_property("s", "root", "text", "").unwrap();
    let call = serde_json::to_value(p.drain_outgoing().pop().unwrap()).unwrap();
    p.unmount_property(&id);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["callAgentFunction"]["functionCallId"],"value":"late"}}),
    );
    assert!(p.drain_outgoing().is_empty());
}

#[test]
fn mounted_status_preserves_pending_and_malformed_updated_rule_is_rejected() {
    use a2ui_base::evaluation::EvaluationStatus;
    let mut p = setup();
    send(
        &mut p,
        json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"TextField","checks":[{"condition":{"@call":"check","args":{}},"message":"Unavailable"}]}]}}),
    );
    let id = p.mount_check_rule("s", "root", 0, "").unwrap();
    assert_eq!(p.mounted_status(&id), Some(EvaluationStatus::Pending));
    assert_eq!(p.mounted_value(&id), Some(json!({"valid":true})));
    let sibling = p.mount_property("s", "sibling", "text", "").unwrap();
    let message = json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"TextField","checks":[{"message":"missing condition"}]},{"id":"sibling","component":"Text","text":{"@path":"/name"}}]}});
    assert!(
        p.process_json_message(&message.to_string())
            .unwrap_err()
            .to_string()
            .contains("check requires condition")
    );
    assert_eq!(p.mounted_status(&id), None);
    assert_eq!(
        p.mounted_value(&sibling),
        Some(json!("Ada")),
        "bad check must not stall sibling expression"
    );
    assert!(
        p.drain_outgoing().is_empty(),
        "cancelled unsent check removed"
    );
}

#[test]
fn current_check_prefix_preserves_structured_metadata() {
    let mut p = setup();
    send(
        &mut p,
        json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"TextField","@checks":[{"condition":{"valid":false,"message":"native message","severity":"warning","code":"WARN","extra":42},"message":"fallback"}],"checks":[{"condition":true}]}]}}),
    );
    let id = p.mount_check_rule("s", "root", 0, "").unwrap();
    assert_eq!(
        p.mounted_value(&id),
        Some(
            json!({"valid":false,"message":"native message","severity":"warning","code":"WARN","extra":42})
        )
    );
}
