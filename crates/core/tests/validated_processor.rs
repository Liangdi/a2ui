use a2ui_base::{
    catalog::SchemaBackedCatalog,
    protocol::validated_processor::{IngressError, ValidatedMessageProcessor},
};
use serde_json::json;
fn runtime() -> ValidatedMessageProcessor {
    let basic = SchemaBackedCatalog::basic().unwrap();
    let id = basic.definition().catalog_id().to_string();
    ValidatedMessageProcessor::new(vec![basic], &id).unwrap()
}
#[test]
fn protected_ingress_rejects_without_state_drift() {
    let mut p = runtime();
    p.process_json_message(&json!({"version":"1.0","createSurface":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["leaf"]},{"id":"leaf","component":"Text","text":"original"}],"dataModel":{"value":1}}}).to_string()).unwrap();
    assert!(matches!(p.process_json_message(&json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["missing"]}]}}).to_string()),Err(IngressError::Validation(_))));
    assert_eq!(
        p.component("s", "root").unwrap().properties["children"],
        json!(["leaf"])
    );
    // Duplicate create passes structural validation but the runtime refuses it.
    assert!(matches!(p.process_json_message(&json!({"version":"1.0","createSurface":{"surfaceId":"s","components":[{"id":"root","component":"Text","text":"replacement"}]}}).to_string()),Err(IngressError::Processing(_))));
    p.process_json_message(&json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"leaf","component":"Text","text":"updated"}]}}).to_string()).unwrap();
    assert_eq!(
        p.component("s", "leaf").unwrap().properties["text"],
        "updated"
    );
    assert_eq!(p.data("s", "/value"), Some(json!(1)));
    // Owned snapshot cannot mutate the protected model.
    let mut snapshot = p.component("s", "root").unwrap();
    snapshot.properties.insert("children".into(), json!([]));
    assert_eq!(
        p.component("s", "root").unwrap().properties["children"],
        json!(["leaf"])
    );
}
#[test]
fn current_only_and_explicit_default() {
    let mut p = runtime();
    assert!(
        p.process_json_message(r#"{"version":"v1.0","createSurface":{"surfaceId":"s"}}"#)
            .is_err()
    );
    p.process_json_message(r#"{"version":"1.0","createSurface":{"surfaceId":"s"}}"#)
        .unwrap();
    p.process_json_message(r#"{"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Text","text":"ok"}]}}"#).unwrap();
    assert!(p.component("s", "root").is_some());
}

#[test]
fn checked_ingress_bootstrap_and_resync() {
    use a2ui_base::protocol::validated_processor::CheckedIngress;
    let basic = SchemaBackedCatalog::basic().unwrap();
    let id = basic.definition().catalog_id().to_owned();
    let definition = basic.definition().clone();
    let (mut guard, mut p) = CheckedIngress::bootstrap(vec![basic], &id).unwrap();
    guard.process_json_message(&mut p,&json!({"version":"1.0","createSurface":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["leaf"]},{"id":"leaf","component":"Text","text":"initial"}]}}).to_string()).unwrap();
    assert!(CheckedIngress::new(vec![definition], &id, &p).is_err());
    // Simulate a trusted legacy host mutation. Checked ingress must not use its
    // previous graph snapshot when deciding if a subsequent update is safe.
    p.model
        .get_surface("s")
        .unwrap()
        .components
        .borrow_mut()
        .get_mut("root")
        .unwrap()
        .properties
        .insert("children".into(), json!(["missing"]));
    let update = json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"leaf","component":"Text","text":"must not commit"}]}});
    assert!(
        guard
            .process_json_message(&mut p, &update.to_string())
            .is_err()
    );
    assert_eq!(
        p.model
            .get_surface("s")
            .unwrap()
            .components
            .borrow()
            .get("leaf")
            .unwrap()
            .properties["text"],
        "initial"
    );
    let repair = json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Column","children":["leaf"]}]}});
    guard
        .process_json_message(&mut p, &repair.to_string())
        .unwrap();
    guard
        .process_json_message(&mut p, &update.to_string())
        .unwrap();
}
