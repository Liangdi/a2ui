use a2ui_base::catalog::basic_functions::{AndFunction, OpenUrlFunction};
use a2ui_base::catalog::{Catalog, CatalogDefinition, SchemaBackedCatalog};
use a2ui_base::model::{data_context::DataContext, data_model::DataModel};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn basic_has_real_native_functions_and_validating_components() {
    let basic = SchemaBackedCatalog::basic().unwrap();
    assert_eq!(basic.catalog().components.len(), 18);
    assert_eq!(basic.catalog().functions.len(), 14);
    let text = basic.catalog().get_component("Text").unwrap();
    assert!(
        text.validate_properties(json!({"text":"Hello"}).as_object().unwrap())
            .is_ok()
    );
    assert!(
        text.validate_properties(json!({"text":42}).as_object().unwrap())
            .is_err()
    );
    let model = DataModel::new();
    let functions = HashMap::new();
    let context = DataContext::new(&model, &functions);
    let function = basic.catalog().get_function("and").unwrap();
    assert_eq!(
        function
            .execute(
                &HashMap::from([("values".into(), json!([true, false]))]),
                &context
            )
            .unwrap(),
        json!(false)
    );
    assert!(
        basic
            .catalog()
            .get_function("openUrl")
            .unwrap()
            .requires_user_activation()
    );
}
#[test]
fn binding_rejects_missing_extra_or_conflicting_native_policies() {
    let doc = json!({"catalogId":"custom","protocolVersion":"1.0","functions":{"and":{"returnType":"boolean"}}});
    let definition = CatalogDefinition::from_json(doc.clone(), None, None).unwrap();
    assert!(SchemaBackedCatalog::new(Catalog::new("custom"), definition.clone()).is_err());
    assert!(
        SchemaBackedCatalog::new(
            Catalog::new("custom").with_function(Box::new(AndFunction)),
            definition.clone()
        )
        .is_ok()
    );
    assert!(
        SchemaBackedCatalog::new(
            Catalog::new("other").with_function(Box::new(AndFunction)),
            definition.clone()
        )
        .is_err()
    );
    assert!(
        SchemaBackedCatalog::new(
            Catalog::new("custom")
                .with_function(Box::new(AndFunction))
                .with_function(Box::new(OpenUrlFunction)),
            definition
        )
        .is_err()
    );
    let mut wrong = doc;
    wrong["functions"]["and"]["returnType"] = json!("string");
    assert!(
        SchemaBackedCatalog::new(
            Catalog::new("custom").with_function(Box::new(AndFunction)),
            CatalogDefinition::from_json(wrong, None, None).unwrap()
        )
        .is_err()
    );
}

#[test]
fn binding_rejects_policy_drift_and_schema_only_execution() {
    use a2ui_base::catalog::SchemaOnlyFunction;
    use a2ui_base::catalog::function_api::ReturnType;
    for policy in [
        json!({"allowedCallers":"agentOnly"}),
        json!({"requiresUserActivation":true}),
    ] {
        let mut function = json!({"returnType":"boolean"});
        function
            .as_object_mut()
            .unwrap()
            .extend(policy.as_object().unwrap().clone());
        let definition = CatalogDefinition::from_json(
            json!({"protocolVersion":"1.0","catalogId":"custom","functions":{"and":function}}),
            None,
            None,
        )
        .unwrap();
        assert!(
            SchemaBackedCatalog::new(
                Catalog::new("custom").with_function(Box::new(AndFunction)),
                definition
            )
            .is_err()
        );
    }
    let definition = CatalogDefinition::from_json(json!({"protocolVersion":"1.0","catalogId":"custom","functions":{"and":{"returnType":"boolean"}}}),None,None).unwrap();
    assert!(
        SchemaBackedCatalog::new(
            Catalog::new("custom").with_function(Box::new(SchemaOnlyFunction::new(
                "and".into(),
                ReturnType::Boolean
            ))),
            definition
        )
        .is_err()
    );
}
