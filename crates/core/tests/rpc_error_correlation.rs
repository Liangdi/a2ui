use a2ui_base::{
    catalog::Catalog, message_processor::MessageProcessor,
    protocol::current_schema::CurrentSchemaValidator,
};
use serde_json::{Value, json};
fn send(p: &mut MessageProcessor, value: Value) {
    p.process_json_message(&value.to_string()).unwrap();
}
fn validator() -> CurrentSchemaValidator {
    CurrentSchemaValidator::new(
        serde_json::from_str(include_str!("conformance/catalogs/basic_v1.json")).unwrap(),
    )
    .unwrap()
}
#[test]
fn identical_errors_across_calls_and_surfaces_keep_exact_identity() {
    let mut p = MessageProcessor::new(vec![Catalog::new("c")]);
    for surface in ["first", "second"] {
        send(
            &mut p,
            json!({"version":"1.0","createSurface":{"surfaceId":surface,"catalogId":"c"}}),
        );
        for n in 0..2 {
            p.start_current_action(surface,&format!("button{n}"),"",json!({"event":{"name":"test","context":{"value":{"@call":"remote","args":{"n":n}}}}})).unwrap();
        }
    }
    let calls: Vec<Value> = p
        .drain_outgoing()
        .into_iter()
        .map(|message| serde_json::to_value(message).unwrap()["callAgentFunction"].clone())
        .collect();
    assert_eq!(calls.len(), 4);
    for call in &calls {
        send(
            &mut p,
            json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"error":{"code":"EXECUTION_ERROR","message":"offline"}}}),
        );
    }
    let messages = p.drain_outgoing();
    assert_eq!(messages.len(), 4);
    let schema = validator();
    for call in &calls {
        let matches: Vec<_> = messages
            .iter()
            .map(|message| serde_json::to_value(message).unwrap())
            .filter(|message| message["error"]["functionCallId"] == call["functionCallId"])
            .collect();
        assert_eq!(matches.len(), 1);
        let message = &matches[0];
        schema.validate_outgoing(message).unwrap();
        assert!(message["error"].get("surfaceId").is_none());
        assert_eq!(
            message["error"]["message"],
            "Action event context entry 'value' is invalid: offline"
        );
    }
    let diagnostics = p.drain_action_errors();
    assert_eq!(diagnostics.len(), 4);
    for surface in ["first", "second"] {
        assert_eq!(
            diagnostics
                .iter()
                .filter(|error| error.surface_id == surface)
                .count(),
            2
        );
    }
    // Replayed failures cannot create another error or mutate an unrelated result.
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":calls[0]["functionCallId"],"error":{"code":"EXECUTION_ERROR","message":"offline"}}}),
    );
    assert!(p.drain_outgoing().is_empty());
}
#[test]
fn local_action_errors_use_surface_correlation_only() {
    let mut p = MessageProcessor::new(vec![Catalog::new("c")]);
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"c"}}),
    );
    p.start_current_action(
        "s",
        "button",
        "",
        json!({"event":{"name":"test","userMessage":true}}),
    )
    .unwrap();
    let messages = p.drain_outgoing();
    assert_eq!(messages.len(), 1);
    let value = serde_json::to_value(&messages[0]).unwrap();
    validator().validate_outgoing(&value).unwrap();
    assert_eq!(value["error"]["surfaceId"], "s");
    assert!(value["error"].get("functionCallId").is_none());
}
