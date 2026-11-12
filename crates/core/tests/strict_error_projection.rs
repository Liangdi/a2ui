use a2ui_base::protocol::{
    current_schema::CurrentSchemaValidator, strict_current::StrictValidationError,
};
use serde_json::json;
fn validator() -> CurrentSchemaValidator {
    CurrentSchemaValidator::new(
        serde_json::from_str(include_str!("conformance/catalogs/basic_v1.json")).unwrap(),
    )
    .unwrap()
}
#[test]
fn all_error_codes_and_categories_have_valid_correlated_wire_shapes() {
    let schema = validator();
    for (category, code) in [
        ("ValidationError", "VALIDATION_FAILED"),
        ("IntegrityError", "UNALLOWED_PARENT"),
        ("ValidationError", "UNALLOWED_CHILD"),
        ("RecursionError", "RECURSION_LIMIT_EXCEEDED"),
        ("StateError", "STATE_ERROR"),
    ] {
        let mut error = StrictValidationError {
            category: category.into(),
            code: code.into(),
            message: "Invalid input".into(),
            surface_id: Some("s".into()),
            path: "/components/root/children".into(),
        };
        let wire = error.renderer_error().unwrap();
        schema.validate_outgoing(&wire).unwrap();
        if matches!(
            code,
            "VALIDATION_FAILED" | "UNALLOWED_PARENT" | "UNALLOWED_CHILD"
        ) {
            assert_eq!(wire["error"]["path"], "");
        } else {
            assert!(wire["error"].get("path").is_none());
        }
        error.surface_id = None;
        assert!(
            error.renderer_error().is_none(),
            "uncorrelated {category} must stay local"
        );
        let source = json!({"version":"1.0","callRendererFunction":{"functionCallId":"c","callFunction":{"@call":"unknown"}}});
        let correlated = error.renderer_error_for_message(&source).unwrap();
        schema.validate_outgoing(&correlated).unwrap();
        assert_eq!(correlated["error"]["functionCallId"], "c");
        assert!(correlated["error"].get("surfaceId").is_none());
        assert!(correlated["error"].get("path").is_none());
        assert_eq!(
            error.path, "/components/root/children",
            "local graph details retained"
        );
    }
}
#[test]
fn payload_paths_are_verified_not_guessed_from_graph_ids() {
    let schema = validator();
    let source = json!({"version":"1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Text","text":7}]}});
    let mut error = StrictValidationError {
        category: "ValidationError".into(),
        code: "VALIDATION_FAILED".into(),
        message: "Invalid text".into(),
        surface_id: Some("s".into()),
        path: "/components/0/text".into(),
    };
    let wire = error.renderer_error_for_message(&source).unwrap();
    schema.validate_outgoing(&wire).unwrap();
    assert_eq!(wire["error"]["path"], "/components/0/text");
    for path in [
        "/components/root/text",
        "/components/0/missing",
        "/components/3/text",
    ] {
        error.path = path.into();
        assert_eq!(
            error.renderer_error_for_message(&source).unwrap()["error"]["path"],
            ""
        );
    }
    error.path = "/components/0/text".into();
    error.surface_id = Some("different".into());
    assert_eq!(
        error.renderer_error_for_message(&source).unwrap()["error"]["path"],
        ""
    );
    error.surface_id = None;
    assert_eq!(
        error.renderer_error_for_message(&source).unwrap()["error"]["surfaceId"],
        "s"
    );
    assert!(error.renderer_error_for_message(&json!({"version":"1.0","createSurface":{"surfaceId":"a"},"deleteSurface":{"surfaceId":"b"}})).is_none());
    assert!(
        error
            .renderer_error_for_message(&json!({"version":"1.0"}))
            .is_none()
    );
    assert!(error.renderer_error_for_message(&json!({"version":"1.0","createSurface":{"functionCallId":"not_a_valid_correlation"}})).is_none());
    assert!(error.renderer_error_for_message(&json!({"version":"1.0","agentFunctionResponse":{"surfaceId":"not_a_valid_correlation"}})).is_none());
}
