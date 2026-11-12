//! Pinned official function corpus. This is a scoped audit, not full conformance.
use a2ui_base::{
    catalog::{basic_functions::build_basic_functions, function_api::FunctionImplementation},
    model::{data_context::DataContext, data_model::DataModel},
};
use serde_json::{Value, json};
use std::collections::HashMap;

#[test]
fn official_function_corpus() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/functions.yaml")).unwrap();
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().into(), f))
        .collect();
    let data = DataModel::new();
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/functions-coverage.json")).unwrap();
    let mut known: HashMap<&str, &Value> = manifest["known_divergences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["name"].as_str().unwrap(), v))
        .collect();
    let omitted: HashMap<&str, &Value> = manifest["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["name"].as_str().unwrap(), v))
        .collect();
    let mut passed = 0;
    let unsupported = 0;
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        if case["action"] == "validate" {
            match validate_surface_case(case) {
                Ok(()) => passed += 1,
                Err(error) => failures.push(
                    json!({"name":name,"actual":error.to_string(),"expected":case["expect"]}),
                ),
            }
            continue;
        }
        assert_eq!(
            case["action"], "evaluate_function",
            "unsupported function corpus action: {name}"
        );
        let mut context = DataContext::new(&data, &functions);
        if case["protocolVersion"] == "1.0" {
            context = context.with_current_protocol();
        }
        let args = case["args"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let result = if case.get("surface").is_some() {
            evaluate_on_surface(case)
        } else {
            functions[case["function"].as_str().unwrap()].execute(&args, &context)
        };
        let valid = match &result {
            Ok(value) => case.get("expectError").is_none() && *value == case["expect"],
            Err(error) => {
                case.get("expectError").is_some()
                    && case["expectError"]["message"]
                        .as_str()
                        .is_none_or(|text| error.to_string().contains(text))
            }
        };
        if valid {
            passed += 1;
        } else {
            failures.push(json!({"name":name,"actual":format!("{result:?}"),"expected":case.get("expect").or(case.get("expectError"))}));
        }
    }
    eprintln!(
        "official functions: {passed} passed, {} diverged, {unsupported} unsupported of {}",
        failures.len(),
        cases.len()
    );
    for failure in &failures {
        eprintln!("DIVERGENCE {failure}");
    }
    for failure in &failures {
        let name = failure["name"].as_str().unwrap();
        let documented = known
            .remove(name)
            .unwrap_or_else(|| panic!("new conformance divergence: {failure}"));
        assert_eq!(
            failure["actual"], documented["observed"],
            "divergence changed: {name}"
        );
    }
    assert!(
        known.is_empty(),
        "fixed divergences must be removed from manifest: {:?}",
        known.keys()
    );
    assert!(
        omitted.is_empty(),
        "stale unsupported cases: {:?}",
        omitted.keys()
    );
}

#[test]
fn released_validators_return_structured_results_and_draft_stays_boolean() {
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().into(), f))
        .collect();
    let data = DataModel::new();
    for (name, args, valid) in [
        ("required", json!({"value":""}), false),
        ("regex", json!({"value":"abc","pattern":"^a"}), true),
        ("length", json!({"value":[1,2],"min":2}), true),
        ("numeric", json!({"value":"NaN"}), false),
        ("email", json!({"value":"a@b.c"}), false),
    ] {
        let args = args
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let legacy = DataContext::new(&data, &functions);
        let current = DataContext::new(&data, &functions).with_current_protocol();
        assert_eq!(
            functions[name].execute(&args, &legacy).unwrap(),
            json!(valid)
        );
        assert_eq!(
            functions[name].execute(&args, &current).unwrap(),
            json!({"valid":valid})
        );
        assert_eq!(
            functions[name]
                .execute(&args, &current.nested("rows/0"))
                .unwrap(),
            json!({"valid":valid})
        );
    }
}

fn evaluate_on_surface(case: &Value) -> Result<Value, a2ui_base::error::A2uiError> {
    use a2ui_base::catalog::{Catalog, SchemaOnlyFunction, function_api::AllowedCallers};
    use a2ui_base::protocol::common_types::FunctionCall;
    let mut catalogs = HashMap::new();
    let mut versions = HashMap::new();
    for path in case["catalogPaths"].as_array().unwrap() {
        let raw = match path.as_str().unwrap() {
            "catalogs/basic/v1/catalog.json" => include_str!("conformance/catalogs/basic_v1.json"),
            "specification/v0_9/catalogs/basic/catalog.json" => {
                include_str!("conformance/catalogs/basic_v09.json")
            }
            "conformance/test_data/catalogs/custom_catalog_v1_0.json" => {
                include_str!("conformance/catalogs/custom_catalog_v1_0.json")
            }
            "conformance/test_data/catalogs/allowed_callers_catalog_v1_0.json" => {
                include_str!("conformance/catalogs/allowed_callers_catalog_v1_0.json")
            }
            other => panic!("unhandled official catalog fixture: {other}"),
        };
        let json: Value = serde_json::from_str(raw).unwrap();
        let parsed = a2ui_base::capabilities::parse_inline_catalog(&json)?;
        let mut catalog = Catalog::new(&parsed.catalog_id);
        let mut native: HashMap<_, _> = build_basic_functions()
            .into_iter()
            .map(|f| (f.name().to_owned(), f))
            .collect();
        for declared in parsed.functions {
            if let Some(function) = native.remove(&declared.name) {
                catalog = catalog.with_function(function);
            } else {
                let declaration = &json["functions"][&declared.name];
                let caller = match declaration["allowedCallers"].as_str() {
                    Some("agentOnly") => AllowedCallers::AgentOnly,
                    Some("rendererOrAgent") => AllowedCallers::RendererOrAgent,
                    _ => AllowedCallers::RendererOnly,
                };
                let function = SchemaOnlyFunction::new(
                    declared.name,
                    a2ui_base::catalog::schema_only::parse_return_type(&declared.return_type),
                )
                .with_policy(
                    json!({}),
                    caller,
                    declaration["requiresUserActivation"]
                        .as_bool()
                        .unwrap_or(false),
                )?;
                catalog = catalog.with_function(Box::new(function));
            }
        }
        versions.insert(
            parsed.catalog_id.clone(),
            json["protocolVersion"]
                .as_str()
                .unwrap_or("v0.9")
                .to_owned(),
        );
        catalogs.insert(parsed.catalog_id, catalog);
    }
    let data = DataModel::from_value(case.get("dataModel").cloned().unwrap_or(json!({})));
    let empty = HashMap::new();
    let mut context = DataContext::new(&data, &empty)
        .with_catalogs(&catalogs, case["surface"]["catalogId"].as_str())
        .with_catalog_versions(&versions);
    if case["protocolVersion"] == "1.0" {
        context = context.with_current_protocol();
    }
    context.try_execute_function(&FunctionCall {
        call: case["function"].as_str().unwrap().into(),
        catalog_id: case["catalogId"].as_str().map(str::to_owned),
        args: case["args"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    })
}

fn validate_surface_case(case: &Value) -> Result<(), a2ui_base::error::A2uiError> {
    use a2ui_base::error::A2uiError;
    use a2ui_base::{catalog::Catalog, message_processor::MessageProcessor};
    let mut catalog = Catalog::new("basic");
    for function in build_basic_functions() {
        catalog = catalog.with_function(function);
    }
    let mut processor = MessageProcessor::new(vec![catalog]);
    let mut resolved = json!({"surfaces":{}});
    for step in case["steps"].as_array().unwrap() {
        let result = (|| -> Result<(), A2uiError> {
            for message in step["payload"].as_array().unwrap() {
                processor
                    .process_message(MessageProcessor::parse_message(&message.to_string())?)?;
            }
            for surface in processor.model.surfaces() {
                let data = surface.data_model.borrow();
                let catalog = processor.catalog(&surface.catalog_id).unwrap();
                let mut context = DataContext::new(&data, &catalog.functions)
                    .with_catalog_id(&surface.catalog_id);
                if surface.current_protocol {
                    context = context.with_current_protocol();
                }
                for (id, component) in surface.components.borrow().all().iter() {
                    let mut properties = serde_json::Map::new();
                    properties.insert("component".into(), json!(component.component_type));
                    for (key, value) in &component.properties {
                        properties.insert(key.clone(), context.resolve_dynamic_json(value)?);
                    }
                    resolved["surfaces"][&surface.id]["components"][id] = Value::Object(properties);
                }
            }
            Ok(())
        })();
        if step.get("expectError").is_some() {
            if result.is_ok() {
                return Err(A2uiError::Validation(
                    "expected evaluation error was not raised".into(),
                ));
            }
        } else {
            result?;
        }
    }
    if let Some(expected) = case.get("expect") {
        assert_subset(expected, &resolved)?;
    }
    Ok(())
}
fn assert_subset(expected: &Value, actual: &Value) -> Result<(), a2ui_base::error::A2uiError> {
    if let Some(object) = expected.as_object() {
        for (key, value) in object {
            assert_subset(value, &actual[key])?;
        }
        Ok(())
    } else if expected == actual {
        Ok(())
    } else {
        Err(a2ui_base::error::A2uiError::Validation(format!(
            "expected {expected}, got {actual}"
        )))
    }
}
