use a2ui_base::catalog::CatalogDefinition;
use serde_json::Value;
#[test]
fn official_current_catalog_corpus() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/catalog.yaml")).unwrap();
    let mut passed = 0;
    let mut failures = vec![];
    let mut skipped = vec![];
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let action = case["action"].as_str().unwrap();
        let raw = if let Some(raw) = case.get("catalog") {
            raw.clone()
        } else if let Some(path) = case["catalogPath"].as_str() {
            let file = if path.starts_with("catalogs/") && path.contains("/v1/") {
                format!("{}.json", path.split('/').nth(1).unwrap())
            } else if path.starts_with("conformance/test_data/catalogs/")
                && path.ends_with("v1_0.json")
            {
                path.rsplit('/').next().unwrap().into()
            } else {
                skipped.push((name.to_owned(), "non-current catalog source"));
                continue;
            };
            serde_json::from_str(
                &std::fs::read_to_string(format!(
                    "{}/tests/conformance/catalog-definitions/{file}",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .unwrap(),
            )
            .unwrap()
        } else {
            Value::Null
        };
        let version = case["protocolVersion"]
            .as_str()
            .or_else(|| raw["protocolVersion"].as_str())
            .unwrap_or("v0.9")
            .to_owned();
        if version.trim_start_matches('v') != "1.0" {
            skipped.push((
                name.to_owned(),
                "legacy catalog API outside current definition scope",
            ));
            continue;
        }
        if !matches!(
            action,
            "from_json" | "round_trip" | "derive" | "validation_schema"
        ) {
            skipped.push((
                name.to_owned(),
                "native SDK serialization not implemented",
            ));
            continue;
        }
        if case["expect"]["matchesBuiltinCatalog"] == true {
            skipped.push((
                name.to_owned(),
                "native SDK built-in equivalence not established",
            ));
            continue;
        }
        let result = run(&case, raw, &version);
        match result {
            Ok(()) => {
                passed += 1;
                eprintln!("[PASS] {name}");
            }
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    for (name, reason) in &skipped {
        eprintln!("[UNSUPPORTED] {name}: {reason}");
    }
    eprintln!(
        "{passed} passed, {} failed, {} explicit exclusions",
        failures.len(),
        skipped.len()
    );
    let manifest: Value =
        serde_json::from_str(include_str!("conformance/catalog-coverage.json")).unwrap();
    let actual: Vec<Value> = skipped
        .iter()
        .map(|(name, reason)| serde_json::json!({"name":name,"reason":reason}))
        .collect();
    assert_eq!(
        manifest["excluded"],
        serde_json::json!(actual),
        "catalog exclusion manifest must match exactly"
    );
    assert_eq!(
        manifest["passed"], passed,
        "catalog executed-case count changed"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
fn run(case: &Value, raw: Value, version: &str) -> Result<(), String> {
    let parsed =
        CatalogDefinition::from_json(raw.clone(), case["catalogId"].as_str(), Some(version))
            .and_then(|catalog| {
                if case["action"] == "derive" {
                    catalog.derive(&case["args"])
                } else {
                    Ok(catalog)
                }
            });
    if let Some(error) = case.get("expectError") {
        let error_actual = parsed
            .err()
            .ok_or("expected error but accepted")?
            .to_string();
        if let Some(message) = error["message"].as_str() {
            if !error_actual.contains(message) {
                return Err(format!("error mismatch {error_actual}"));
            }
        }
        return Ok(());
    }
    let parsed = parsed.map_err(|e| e.to_string())?;
    if case["action"] == "validation_schema" {
        let file = case["expectFile"]
            .as_str()
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap();
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(format!(
                "{}/tests/conformance/validation-schema/{file}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        let actual = parsed.validation_schema().map_err(|e| e.to_string())?;
        fn canonical(value: &mut Value) {
            match value {
                Value::Object(object) => {
                    for (key, value) in object {
                        canonical(value);
                        if matches!(key.as_str(), "enum" | "required") {
                            if let Some(values) = value.as_array_mut() {
                                values.sort_by_key(Value::to_string);
                            }
                        }
                    }
                }
                Value::Array(values) => {
                    for child in values {
                        canonical(child);
                    }
                }
                _ => {}
            }
        }
        let mut actual = actual;
        let mut expected = expected;
        canonical(&mut actual);
        canonical(&mut expected);
        if actual != expected {
            std::fs::write(
                format!("/tmp/{file}.actual"),
                serde_json::to_string_pretty(&actual).unwrap(),
            )
            .unwrap();
            return Err(format!("canonical schema differs: /tmp/{file}.actual"));
        }
    }
    if case["action"] == "round_trip" {
        if parsed.to_json() != raw {
            return Err("roundtrip changed document".into());
        }
        let second = CatalogDefinition::from_json(parsed.to_json(), None, Some(version))
            .map_err(|e| e.to_string())?;
        if second.to_json() != raw {
            return Err("second roundtrip changed document".into());
        }
    }
    if let Some(expect) = case["expect"].as_object() {
        for (key, value) in expect {
            match key.as_str() {
                "catalogId" => {
                    if value != parsed.catalog_id() {
                        return Err("catalog id mismatch".into());
                    }
                }
                "protocolVersion" => {
                    if value != parsed.protocol_version() {
                        return Err("version mismatch".into());
                    }
                }
                "components" | "functions" => {
                    let actual = if key == "components" {
                        parsed.components()
                    } else {
                        parsed.functions()
                    };
                    let mut names: Vec<String> = if let Some(array) = value.as_array() {
                        array.iter().map(|v| v.as_str().unwrap().into()).collect()
                    } else {
                        value.as_object().unwrap().keys().cloned().collect()
                    };
                    names.sort();
                    if names != actual.keys().cloned().collect::<Vec<_>>() {
                        return Err(format!(
                            "{key} names mismatch {names:?} vs {:?}",
                            actual.keys()
                        ));
                    }
                    if let Some(expected) = value.as_object() {
                        for (name, schema) in expected {
                            if actual[name] != *schema {
                                return Err(format!("{name} schema mismatch"));
                            }
                        }
                    }
                }
                "theme" | "instructions" => {
                    if parsed.to_json().get(key) != Some(value) {
                        return Err(format!("{key} mismatch"));
                    }
                }
                "absentKeys" => {
                    for key in value.as_array().unwrap() {
                        if parsed.to_json().get(key.as_str().unwrap()).is_some() {
                            return Err(format!("unexpected key {key}"));
                        }
                    }
                }
                "componentMetadata" => {
                    for (name, metadata) in value.as_object().unwrap() {
                        if parsed.components()[name]
                            .get("metadata")
                            .unwrap_or(&Value::Null)
                            != metadata
                        {
                            return Err(format!("{name} metadata mismatch"));
                        }
                    }
                }
                "selfContained" => {
                    if value != true {
                        return Err("unknown selfContained value".into());
                    }
                }
                "validComponents" | "invalidComponents" => {
                    for component in value.as_array().unwrap() {
                        let outcome = parsed.validate_component(component);
                        if outcome.is_ok() != (key == "validComponents") {
                            return Err(format!("{key}: {component}: {outcome:?}"));
                        }
                    }
                }
                _ => return Err(format!("unsupported expectation {key}")),
            }
        }
    }
    Ok(())
}
