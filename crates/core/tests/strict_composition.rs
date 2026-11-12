//! Runtime safety coverage for composition constraints, not a full audit of the
//! official composition diagnostic paths or relaxed topology configurations.
use a2ui_base::{
    catalog::definition::CatalogDefinition, protocol::strict_current::StrictCurrentValidator,
};
use serde_json::{Value, json};
fn validator(mut components: Value) -> StrictCurrentValidator {
    for (name, schema) in components.as_object_mut().unwrap() {
        let properties = schema
            .as_object_mut()
            .unwrap()
            .entry("properties")
            .or_insert(json!({}));
        properties["component"] = json!({"const":name});
    }
    let definition = CatalogDefinition::from_json(
        json!({"catalogId":"c","protocolVersion":"1.0","components":components}),
        None,
        None,
    )
    .unwrap();
    let mut validator = StrictCurrentValidator::new();
    validator.register_catalog(definition, true).unwrap();
    validator
}
fn create(components: Value) -> Value {
    json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"c","components":components}})
}
#[test]
fn schema_defined_slot_edges_enforce_child_constraints() {
    let mut validator = validator(
        json!({"Box":{"type":"object","allowedChildren":["Text"],"properties":{"customSlot":{"$ref":"common_types.json#/$defs/ComponentId"}}},"Video":{"type":"object"},"Text":{"type":"object"}}),
    );
    let error=validator.validate_message(&create(json!([{"id":"root","component":"Box","customSlot":"video"},{"id":"video","component":"Video"}]))).unwrap_err();
    assert_eq!(error.code, "UNALLOWED_CHILD", "{error:?}");
    assert_eq!(error.renderer_error()["error"]["code"], "UNALLOWED_CHILD");
    // Failed structural validation must not poison the subsequent valid create.
    validator.validate_message(&create(json!([{"id":"root","component":"Box","customSlot":"text"},{"id":"text","component":"Text"}]))).unwrap();
}
#[test]
fn root_parent_policy_is_checked_against_surface() {
    let mut validator = validator(json!({"Header":{"type":"object","allowedParents":["Box"]}}));
    let error = validator
        .validate_message(&create(json!([{"id":"root","component":"Header"}])))
        .unwrap_err();
    assert_eq!(error.code, "UNALLOWED_PARENT", "{error:?}");
    assert_eq!(error.category, "ValidationError");
    assert!(error.message.contains("Surface"));
}
#[test]
fn every_schema_derived_parent_is_checked() {
    let children = json!({"$ref":"common_types.json#/$defs/ChildList"});
    let mut validator = validator(
        json!({"Box":{"type":"object","properties":{"children":children}},"Card":{"type":"object","properties":{"children":children}},"Header":{"type":"object","allowedParents":["Box"]}}),
    );
    let error=validator.validate_message(&create(json!([{"id":"root","component":"Box","children":["box","card"]},{"id":"box","component":"Box","children":["h"]},{"id":"card","component":"Card","children":["h"]},{"id":"h","component":"Header"}]))).unwrap_err();
    assert_eq!(error.code, "UNALLOWED_PARENT", "{error:?}");
    assert!(error.message.contains("'card' (Card)"));
}
