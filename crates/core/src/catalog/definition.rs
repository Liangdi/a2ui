//! Authored current catalog documents, distinct from native renderer registrations.
use crate::error::{A2uiError, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct CatalogDefinition {
    raw: Value,
    resolved: Value,
    id: String,
    version: String,
    components: BTreeMap<String, Value>,
    functions: BTreeMap<String, Value>,
}
impl CatalogDefinition {
    pub fn from_json(mut raw: Value, id: Option<&str>, version: Option<&str>) -> Result<Self> {
        let object = raw
            .as_object_mut()
            .ok_or_else(|| invalid("catalog must be an object"))?;
        let id = id
            .or_else(|| object.get("catalogId").and_then(Value::as_str))
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid("catalog_id must be provided or exist in catalog schema"))?
            .to_owned();
        let version = version
            .or_else(|| object.get("protocolVersion").and_then(Value::as_str))
            .unwrap_or("v0.9")
            .to_owned();
        if id
            != object
                .get("catalogId")
                .and_then(Value::as_str)
                .unwrap_or("")
        {
            object.insert("catalogId".into(), json!(id));
        }
        if version.trim_start_matches('v') != "1.0" {
            return Err(invalid(
                "CatalogDefinition currently supports released 1.0 only",
            ));
        }
        validate_document(&raw)?;
        let components = selected(&raw, "components", "anyComponent")?;
        let functions = selected(&raw, "functions", "anyFunction")?;
        let mut resolved = raw.clone();
        rewrite_refs(&mut resolved, false)?;
        let common: Value =
            serde_json::from_str(include_str!("../protocol/schemas/common_types.json"))?;
        let mut common_defs = common["$defs"].clone();
        rewrite_refs(&mut common_defs, true)?;
        // A call naming another catalog cannot be judged against this catalog's
        // function signatures. validate_component performs recursive call checks.
        common_defs["FunctionCall"] = json!({"$ref":"#/$defs/__a2uiCommon/FunctionCommon"});
        let defs = resolved
            .as_object_mut()
            .unwrap()
            .entry("$defs")
            .or_insert(json!({}))
            .as_object_mut()
            .ok_or_else(|| invalid("$defs must be object"))?;
        if defs.contains_key("__a2uiCommon") {
            return Err(invalid("reserved internal definition __a2uiCommon"));
        }
        defs.insert("__a2uiCommon".into(), common_defs);
        for (union, entries, section) in [
            ("anyComponent", &components, "components"),
            ("anyFunction", &functions, "functions"),
        ] {
            if !defs.contains_key(union) {
                defs.insert(union.into(), if entries.is_empty() {json!(false)} else {json!({"oneOf":entries.keys().map(|name|json!({"$ref":format!("#/{section}/{}",escape(name))})).collect::<Vec<_>>()})});
            }
        }
        ensure_local_refs(&resolved, &resolved)?;
        Ok(Self {
            raw,
            resolved,
            id,
            version,
            components,
            functions,
        })
    }
    /// Derive a new authored catalog while retaining reusable definitions and theme.
    pub fn derive(&self, args: &Value) -> Result<Self> {
        let id = args
            .get("catalogId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("derived catalogId required"))?;
        let mut raw = self.raw.clone();
        let object = raw.as_object_mut().unwrap();
        for key in ["$id", "title", "description", "instructions"] {
            object.remove(key);
        }
        object.insert("catalogId".into(), json!(id));
        if let Some(instructions) = args.get("instructions") {
            object.insert("instructions".into(), instructions.clone());
        }
        for (section, source) in [
            ("components", &self.components),
            ("functions", &self.functions),
        ] {
            let mut entries = serde_json::Map::new();
            if let Some(list) = args.get(section) {
                for entry in list
                    .as_array()
                    .ok_or_else(|| invalid("derived entries must be lists"))?
                {
                    if let Some(name) = entry.as_str() {
                        entries.insert(
                            name.into(),
                            source
                                .get(name)
                                .ok_or_else(|| invalid(&format!("unknown entry {name}")))?
                                .clone(),
                        );
                    } else {
                        let addition = entry
                            .as_object()
                            .filter(|o| o.len() == 1)
                            .ok_or_else(|| invalid("new entry must contain exactly one name"))?;
                        entries.extend(addition.clone());
                    }
                }
            } else {
                entries.extend(source.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            object.insert(section.into(), Value::Object(entries));
        }
        if let Some(defs) = object.get_mut("$defs").and_then(Value::as_object_mut) {
            defs.remove("anyComponent");
            defs.remove("anyFunction");
        }
        Self::from_json(raw, None, Some(&self.version))
    }
    pub fn catalog_id(&self) -> &str {
        &self.id
    }
    pub fn protocol_version(&self) -> &str {
        &self.version
    }
    pub fn components(&self) -> &BTreeMap<String, Value> {
        &self.components
    }
    pub fn functions(&self) -> &BTreeMap<String, Value> {
        &self.functions
    }
    pub fn to_json(&self) -> Value {
        self.raw.clone()
    }
    /// Self-contained resource schema. This is not the canonical flattened golden schema.
    pub fn resolved_schema(&self) -> &Value {
        &self.resolved
    }
    pub fn validate_component(&self, value: &Value) -> Result<()> {
        let name = value
            .get("component")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("component name missing"))?;
        if !self.components.contains_key(name) {
            return Err(invalid("component absent from catalog union"));
        }
        let mut schema = self.resolved.clone();
        schema["allOf"] = json!([{"$ref":"#/$defs/__a2uiCommon/ComponentCommon"},{"$ref":format!("#/components/{}",escape(name))}]);
        schema["unevaluatedProperties"] = json!(false);
        jsonschema::options()
            .offline()
            .build(&schema)
            .map_err(|e| invalid(&e.to_string()))?
            .validate(value)
            .map_err(|e| invalid(&e.to_string()))?;
        self.validate_calls(value)
    }
    fn validate_calls(&self, value: &Value) -> Result<()> {
        match value {
            Value::Object(object) => {
                if let Some(call) = object.get("@call") {
                    let name = call
                        .as_str()
                        .ok_or_else(|| invalid("function name must be string"))?;
                    if name != "@index" {
                        validate_identifier(name, "function")?;
                    }
                    if let Some(args) = object.get("args") {
                        for key in args
                            .as_object()
                            .ok_or_else(|| invalid("args must be object"))?
                            .keys()
                        {
                            validate_identifier(key, "argument")?;
                        }
                    }
                    if name == "@index"
                        || object
                            .get("catalogId")
                            .and_then(Value::as_str)
                            .is_none_or(|id| id == self.id)
                    {
                        let reference = if name == "@index" {
                            "#/$defs/__a2uiCommon/IndexSystemFunction".to_string()
                        } else {
                            if !self.functions.contains_key(name) {
                                return Err(invalid("function absent from catalog union"));
                            }
                            format!("#/functions/{}", escape(name))
                        };
                        let mut schema = self.resolved.clone();
                        schema["allOf"] = json!([{"$ref":"#/$defs/__a2uiCommon/FunctionCommon"},{"$ref":reference}]);
                        schema["unevaluatedProperties"] = json!(false);
                        jsonschema::options()
                            .offline()
                            .build(&schema)
                            .map_err(|e| invalid(&e.to_string()))?
                            .validate(value)
                            .map_err(|e| invalid(&e.to_string()))?;
                    }
                }
                for child in object.values() {
                    self.validate_calls(child)?;
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.validate_calls(child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
pub(crate) fn validate_identifier(name: &str, kind: &str) -> Result<()> {
    let mut chars = name.chars();
    if !chars
        .next()
        .is_some_and(|c| c == '_' || unicode_ident::is_xid_start(c))
        || !chars.all(unicode_ident::is_xid_continue)
    {
        return Err(invalid(&format!(
            "Invalid UAX #31 {kind} identifier: {name}"
        )));
    }
    Ok(())
}
fn invalid(message: &str) -> A2uiError {
    A2uiError::Validation(message.into())
}
fn escape(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}
fn validate_document(raw: &Value) -> Result<()> {
    for section in ["components", "functions"] {
        if let Some(entries) = raw.get(section) {
            for (name, entry) in entries
                .as_object()
                .ok_or_else(|| invalid("catalog entries must be objects"))?
            {
                validate_identifier(
                    name,
                    if section == "components" {
                        "component"
                    } else {
                        "function"
                    },
                )?;
                if let Some(props) = entry.get("properties").and_then(Value::as_object) {
                    for key in props.keys() {
                        if !matches!(
                            key.as_str(),
                            "@call" | "@checks" | "@accessibility" | "@metadata" | "@catalogId"
                        ) {
                            validate_identifier(key, "property")?;
                        }
                    }
                }
                for args in [
                    entry.get("parameters"),
                    entry.pointer("/properties/args/properties"),
                    entry.pointer("/args/properties"),
                ]
                .into_iter()
                .flatten()
                {
                    for key in args
                        .as_object()
                        .ok_or_else(|| invalid("arguments must be objects"))?
                        .keys()
                    {
                        validate_identifier(key, "argument")?;
                    }
                }
                if let Some(extensions) = entry
                    .pointer("/metadata/extensions")
                    .and_then(Value::as_object)
                {
                    for key in extensions.keys() {
                        validate_identifier(key, "metadata extension")?;
                    }
                }
                if section == "functions"
                    && entry.get("requiresUserActivation") == Some(&json!(true))
                    && entry
                        .get("allowedCallers")
                        .and_then(Value::as_str)
                        .unwrap_or("rendererOnly")
                        != "rendererOnly"
                {
                    return Err(invalid(
                        "requiresUserActivation only permits rendererOnly callers",
                    ));
                }
            }
        }
    }
    Ok(())
}
fn selected(raw: &Value, section: &str, union: &str) -> Result<BTreeMap<String, Value>> {
    let entries = raw.get(section).and_then(Value::as_object);
    let Some(definition) = raw.pointer(&format!("/$defs/{union}")) else {
        return Ok(entries
            .into_iter()
            .flat_map(|map| map.iter())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect());
    };
    if definition == &json!(false) || definition.get("not") == Some(&json!({})) {
        return Ok(BTreeMap::new());
    }
    let refs = definition
        .get("oneOf")
        .or_else(|| definition.get("anyOf"))
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("catalog union must list references"))?;
    let mut output = BTreeMap::new();
    for reference in refs {
        let path = reference
            .get("$ref")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("catalog union entry must be reference"))?;
        let name = path
            .strip_prefix(&format!("#/{section}/"))
            .ok_or_else(|| invalid("catalog union reference points outside section"))?
            .replace("~1", "/")
            .replace("~0", "~");
        let value = entries
            .and_then(|entries| entries.get(&name))
            .ok_or_else(|| invalid("catalog union reference missing"))?;
        output.insert(name, value.clone());
    }
    Ok(output)
}
fn rewrite_refs(value: &mut Value, common: bool) -> Result<()> {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get_mut("$ref") {
                let text = reference
                    .as_str()
                    .ok_or_else(|| invalid("$ref must be string"))?;
                let new = if let Some(tail) =
                    text.strip_prefix("common_types.json#/$defs/").or_else(|| {
                        text.strip_prefix(
                            "https://a2ui.org/specification/v1_0/common_types.json#/$defs/",
                        )
                    }) {
                    format!("#/$defs/__a2uiCommon/{tail}")
                } else if common && text == "catalog.json#/$defs/anyFunction" {
                    "#/$defs/anyFunction".into()
                } else if let Some(tail) = text.strip_prefix("#/$defs/") {
                    if common {
                        format!("#/$defs/__a2uiCommon/{tail}")
                    } else {
                        text.into()
                    }
                } else if text.starts_with('#') {
                    text.into()
                } else {
                    return Err(invalid(&format!("Unsupported external $ref: {text}")));
                };
                *reference = json!(new);
            }
            for (key, child) in object {
                if !matches!(
                    key.as_str(),
                    "metadata" | "default" | "examples" | "enum" | "const"
                ) {
                    rewrite_refs(child, common)?;
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                rewrite_refs(child, common)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn ensure_local_refs(root: &Value, value: &Value) -> Result<()> {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                if !reference.starts_with('#') || root.pointer(&reference[1..]).is_none() {
                    return Err(invalid("Unresolved local $ref"));
                }
            }
            for (key, child) in object {
                if !matches!(
                    key.as_str(),
                    "metadata" | "default" | "examples" | "enum" | "const"
                ) {
                    ensure_local_refs(root, child)?;
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                ensure_local_refs(root, child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_annotation_refs_are_not_schema_references() {
        let raw = json!({"catalogId":"custom","protocolVersion":"1.0","components":{"Text":{
            "type":"object","properties":{"component":{"const":"Text"},"text":{"type":"string","default":{"$ref":"https://example.invalid/literal"}}},
            "metadata":{"extensions":{"literal":{"$ref":"https://example.invalid/metadata"}}}
        }}});
        let definition = CatalogDefinition::from_json(raw.clone(), None, None).unwrap();
        assert_eq!(definition.to_json(), raw);
    }
    #[test]
    fn current_inline_catalog_uses_unicode_and_union_selection() {
        let raw = json!({"catalogId":"custom","protocolVersion":"1.0","components":{"A_ℓ":{"type":"object"},"Hidden":{"type":"object"}},
            "functions":{"format_ℓ":{"returnType":"string"},"hidden":{"returnType":"string"}},
            "$defs":{"anyComponent":{"oneOf":[{"$ref":"#/components/A_ℓ"}]},"anyFunction":{"oneOf":[{"$ref":"#/functions/format_ℓ"}]}}
        });
        let parsed = crate::capabilities::parse_inline_catalog(&raw).unwrap();
        assert_eq!(parsed.component_names, vec!["A_ℓ"]);
        assert_eq!(parsed.functions.len(), 1);
        assert_eq!(parsed.functions[0].name, "format_ℓ");
    }
}
