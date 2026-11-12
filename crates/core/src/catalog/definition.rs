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
        check_document_depth(&raw)?;
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
        if version != "1.0" {
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
    /// Return a standalone, local-only schema for one selected function's arguments.
    /// The bundled resource paths preserve common types and authored local refs.
    pub fn function_args_schema(&self, name: &str) -> Result<Value> {
        if !self.functions.contains_key(name) {
            return Err(invalid(&format!(
                "function absent from catalog union: {name}"
            )));
        }
        let declaration = &self.resolved["functions"][name];
        let suffix = if declaration.pointer("/properties/args").is_some() {
            "properties/args"
        } else if declaration.get("args").is_some() {
            "args"
        } else {
            return Ok(json!({"type":"object"}));
        };
        let mut schema = json!({
            "$schema":"https://json-schema.org/draft/2020-12/schema",
            "$ref":format!("#/functions/{}/{suffix}",escape(name)),
            "$defs":self.resolved["$defs"].clone(),
            "functions":self.resolved.get("functions").cloned().unwrap_or(json!({})),
            "components":self.resolved.get("components").cloned().unwrap_or(json!({}))
        });
        // An authored local resource may refer to other top-level definitions.
        // Preserve them without turning catalog metadata into validation keywords.
        if let Some(definitions) = self.resolved.get("definitions") {
            schema["definitions"] = definitions.clone();
        }
        ensure_local_refs(&schema, &schema)?;
        Ok(schema)
    }

    /// Canonical flattened, self-contained current catalog validation schema.
    /// Authored leaf definitions are preserved; only referenced common types are bundled.
    pub fn validation_schema(&self) -> Result<Value> {
        let mut common: Value =
            serde_json::from_str(include_str!("../protocol/schemas/common_types.json"))?;
        normalize_validation_refs(&mut common);
        let standard = common["$defs"].as_object().unwrap();
        let mut definitions = self
            .raw
            .get("$defs")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        definitions.remove("anyComponent");
        definitions.remove("anyFunction");
        let mut output = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","catalogId":self.id,"protocolVersion":"1.0"});
        if let Some(instructions) = self.raw.get("instructions") {
            output["instructions"] = instructions.clone();
        }
        if let Some(theme) = self.raw.get("theme") {
            definitions.insert("theme".into(), theme.clone());
        }
        let mut local = Value::Object(definitions.clone());
        normalize_validation_refs(&mut local);
        let mut used_mixins = std::collections::BTreeSet::new();
        for (section, entries) in [
            ("components", &self.components),
            ("functions", &self.functions),
        ] {
            if entries.is_empty() {
                continue;
            }
            let mut values = serde_json::Map::new();
            for (name, entry) in entries {
                let mut value = entry.clone();
                normalize_validation_refs(&mut value);
                if section == "components" {
                    value = flatten_component(name, &value, &local, standard, &mut used_mixins)?;
                } else if let Some(args) = value
                    .pointer_mut("/properties/args")
                    .and_then(Value::as_object_mut)
                {
                    if args.get("type") == Some(&json!("object")) && !args.contains_key("$ref") {
                        let closure = args.remove("additionalProperties").unwrap_or(json!(false));
                        args.entry("unevaluatedProperties").or_insert(closure);
                    }
                }
                propagate_descriptions(&mut value, standard);
                values.insert(name.clone(), value);
            }
            output[section] = Value::Object(values);
            let union = if section == "components" {
                "anyComponent"
            } else {
                "anyFunction"
            };
            // Preserve authored union order; it carries no semantics but improves stability.
            let mut selected_order: Vec<String> = self
                .raw
                .get(section)
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|o| o.keys())
                .filter(|n| entries.contains_key(*n))
                .cloned()
                .collect();
            selected_order.extend(
                entries
                    .keys()
                    .filter(|n| {
                        !self
                            .raw
                            .get(section)
                            .and_then(Value::as_object)
                            .is_some_and(|o| o.contains_key(*n))
                    })
                    .cloned(),
            );
            let mut union_value = json!({"oneOf":selected_order.iter().map(|name|json!({"$ref":format!("#/{section}/{}",escape(name))})).collect::<Vec<_>>()});
            if section == "components" {
                union_value["discriminator"] = json!({"propertyName":"component"});
            }
            definitions.insert(union.into(), union_value);
        }
        let mut local = Value::Object(definitions);
        normalize_validation_refs(&mut local);
        propagate_descriptions(&mut local, standard);
        let definitions = local.as_object_mut().unwrap();
        let mut seeds = std::collections::BTreeSet::new();
        collect_definition_refs(&output, &mut seeds);
        collect_definition_refs(&Value::Object(definitions.clone()), &mut seeds);
        // Flattened-only mixins are not needed unless referenced by surviving leaves.
        for mixin in used_mixins {
            if !seeds.contains(&mixin) {
                definitions.remove(&mixin);
            }
        }
        let mut queue: std::collections::VecDeque<_> = seeds.into_iter().collect();
        while let Some(name) = queue.pop_front() {
            if definitions.contains_key(&name) {
                continue;
            }
            let value = if let Some(value) = standard.get(&name) {
                value.clone()
            } else if matches!(name.as_str(), "anyFunction" | "anyComponent") {
                json!({"not":{}})
            } else {
                return Err(invalid(&format!(
                    "unresolved validation definition: {name}"
                )));
            };
            let mut references = std::collections::BTreeSet::new();
            collect_definition_refs(&value, &mut references);
            queue.extend(references);
            definitions.insert(name, value);
        }
        output["$defs"] = local;
        ensure_local_refs(&output, &output)?;
        Ok(output)
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
    name.replace('~', "~0")
        .replace('/', "~1")
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub(crate) fn decode_fragment(fragment: &str) -> Option<String> {
    let mut bytes = fragment.bytes();
    let mut output = Vec::new();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let a = (bytes.next()? as char).to_digit(16)?;
            let b = (bytes.next()? as char).to_digit(16)?;
            output.push((a * 16 + b) as u8);
        } else {
            output.push(byte);
        }
    }
    String::from_utf8(output).ok()
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
        let decoded = decode_fragment(path).ok_or_else(|| invalid("Invalid URI fragment"))?;
        let name = decoded
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
                if !reference.starts_with('#')
                    || decode_fragment(&reference[1..])
                        .and_then(|p| root.pointer(&p))
                        .is_none()
                {
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

// Bound recursion before walking caller-constructed JSON values (not just parsed text).
fn check_document_depth(root: &Value) -> Result<()> {
    let mut queue = vec![(root, 0usize)];
    while let Some((value, depth)) = queue.pop() {
        if depth > 128 {
            return Err(invalid("catalog document nesting exceeds 128 levels"));
        }
        match value {
            Value::Object(values) => queue.extend(values.values().map(|v| (v, depth + 1))),
            Value::Array(values) => queue.extend(values.iter().map(|v| (v, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}

// Canonicalization follows the published catalog validation-schema contract.
fn normalize_validation_refs(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get_mut("$ref") {
                if let Some(text) = reference.as_str() {
                    let tail = text
                        .strip_prefix("common_types.json#/$defs/")
                        .or_else(|| {
                            text.strip_prefix(
                                "https://a2ui.org/specification/v1_0/common_types.json#/$defs/",
                            )
                        })
                        .or_else(|| text.strip_prefix("catalog.json#/$defs/"));
                    if let Some(tail) = tail {
                        *reference = json!(format!("#/$defs/{tail}"));
                    }
                }
            }
            for (key, child) in object {
                if !matches!(
                    key.as_str(),
                    "metadata" | "default" | "examples" | "enum" | "const"
                ) {
                    normalize_validation_refs(child);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                normalize_validation_refs(child);
            }
        }
        _ => {}
    }
}
fn collect_definition_refs(value: &Value, out: &mut std::collections::BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/$defs/"))
            {
                out.insert(name.split('/').next().unwrap().to_owned());
            }
            for (key, child) in object {
                if !matches!(
                    key.as_str(),
                    "metadata" | "default" | "examples" | "enum" | "const"
                ) {
                    collect_definition_refs(child, out);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_definition_refs(child, out);
            }
        }
        _ => {}
    }
}
fn propagate_descriptions(value: &mut Value, standard: &serde_json::Map<String, Value>) {
    match value {
        Value::Object(object) => {
            if !object.contains_key("description") {
                if let Some(description) = object
                    .get("$ref")
                    .and_then(Value::as_str)
                    .and_then(|r| r.strip_prefix("#/$defs/"))
                    .and_then(|name| standard.get(name))
                    .and_then(|v| v.get("description"))
                    .cloned()
                {
                    object.insert("description".into(), description);
                }
            }
            for (key, child) in object {
                if !matches!(
                    key.as_str(),
                    "metadata" | "default" | "examples" | "enum" | "const"
                ) {
                    propagate_descriptions(child, standard);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                propagate_descriptions(child, standard);
            }
        }
        _ => {}
    }
}
fn flatten_component(
    name: &str,
    entry: &Value,
    local: &Value,
    standard: &serde_json::Map<String, Value>,
    used: &mut std::collections::BTreeSet<String>,
) -> Result<Value> {
    struct Pieces {
        values: Vec<(u8, Value)>,
        residual: Vec<Value>,
        visiting: std::collections::BTreeSet<String>,
    }
    fn collect(
        entry: &Value,
        local: &Value,
        standard: &serde_json::Map<String, Value>,
        used: &mut std::collections::BTreeSet<String>,
        pieces: &mut Pieces,
        priority: u8,
        nested: bool,
    ) -> Result<()> {
        if pieces.visiting.len() > 128 {
            return Err(invalid("component mixin nesting exceeds limit"));
        }
        if let Some(parts) = entry.get("allOf").and_then(Value::as_array) {
            for part in parts {
                if let Some(reference) = part.get("$ref").and_then(Value::as_str) {
                    let name = reference
                        .strip_prefix("#/$defs/")
                        .ok_or_else(|| invalid("unsupported component mixin reference"))?;
                    if !pieces.visiting.insert(name.into()) {
                        return Err(invalid("cyclic component mixin"));
                    }
                    if let Some(target) = local.get(name) {
                        used.insert(name.into());
                        collect(target, local, standard, used, pieces, 1, true)?;
                    } else if let Some(target) = standard.get(name) {
                        pieces.values.push((0, target.clone()));
                    } else {
                        return Err(invalid("unknown component mixin"));
                    }
                    pieces.visiting.remove(name);
                    // Preserve constraints beside a mixin reference rather than silently drop them.
                    let mut siblings = part.clone();
                    siblings.as_object_mut().unwrap().remove("$ref");
                    if siblings.as_object().is_some_and(|o| !o.is_empty()) {
                        collect(&siblings, local, standard, used, pieces, priority, true)?;
                    }
                } else {
                    collect(part, local, standard, used, pieces, priority, true)?;
                }
            }
        }
        if nested {
            // Moving a closed mixin into the containing object changes which sibling
            // fields it can see. Refuse this ambiguous normalization, never drop it.
            for key in ["additionalProperties", "unevaluatedProperties"] {
                if entry.get(key).is_some_and(|v| v != &json!(true)) {
                    return Err(invalid(
                        "closed nested component mixins cannot be flattened safely",
                    ));
                }
            }
            if entry.get("type").is_some_and(|v| v != &json!("object")) {
                return Err(invalid("non-object component mixins cannot be flattened"));
            }
            let mut constraints = entry
                .as_object()
                .cloned()
                .ok_or_else(|| invalid("component mixin must be object"))?;
            for key in [
                "type",
                "description",
                "allOf",
                "properties",
                "required",
                "unevaluatedProperties",
                "additionalProperties",
                "allowedParents",
                "allowedChildren",
            ] {
                constraints.remove(key);
            }
            if !constraints.is_empty() {
                pieces.residual.push(Value::Object(constraints));
            }
        }
        pieces.values.push((priority, entry.clone()));
        Ok(())
    }
    let mut pieces = Pieces {
        values: vec![],
        residual: vec![],
        visiting: Default::default(),
    };
    collect(entry, local, standard, used, &mut pieces, 2, false)?;
    pieces.values.sort_by_key(|(priority, _)| *priority);
    let mut props = serde_json::Map::new();
    let mut required = std::collections::BTreeSet::new();
    let mut description = None;
    let mut component_name = json!(name);
    for (priority, piece) in pieces.values {
        if let Some(value) = piece.get("description") {
            if priority > 0 {
                description = Some(value.clone());
            }
        }
        if let Some(fields) = piece.get("properties").and_then(Value::as_object) {
            if let Some(value) = fields.get("component").and_then(|value| value.get("const")) {
                component_name = value.clone();
            }
            for (key, value) in fields {
                if matches!(key.as_str(), "id" | "component")
                    || (priority == 0 && matches!(key.as_str(), "@catalogId" | "catalogId"))
                {
                    continue;
                }
                props.insert(key.clone(), value.clone());
            }
        }
        if let Some(names) = piece.get("required").and_then(Value::as_array) {
            for name in names {
                if let Some(name) = name.as_str() {
                    if !matches!(name, "id" | "component" | "@catalogId" | "catalogId") {
                        required.insert(name.to_owned());
                    }
                }
            }
        }
    }
    let mut output = entry.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| invalid("component schema must be object"))?;
    object.remove("allOf");
    object.remove("$defs");
    let closure = object.remove("additionalProperties");
    if let Some(closure) = closure {
        object.entry("unevaluatedProperties").or_insert(closure);
    }
    object.insert("type".into(), json!("object"));
    if let Some(description) = description {
        object.insert("description".into(), description);
    }
    if !pieces.residual.is_empty() {
        object.insert("allOf".into(), Value::Array(pieces.residual));
    }
    props.insert("id".into(), json!({"$ref":"#/$defs/ComponentId"}));
    props.insert("component".into(), json!({"const":component_name}));
    object.insert("properties".into(), Value::Object(props));
    object.insert(
        "required".into(),
        json!(
            std::iter::once("id".to_string())
                .chain(required)
                .chain(std::iter::once("component".to_string()))
                .collect::<Vec<_>>()
        ),
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_schema_keeps_inline_constraints_and_precedence() {
        let document = json!({"protocolVersion":"1.0","catalogId":"custom","components":{"Card":{
            "type":"object","allOf":[
                {"properties":{"weight":{"type":"integer","minimum":2}}},
                {"$ref":"#/$defs/Mixin"},
                {"if":{"required":["highlight"]},"then":{"required":["weight"]}}
            ]
        }},"$defs":{"Mixin":{"type":"object","properties":{"weight":{"type":"number"}}}}});
        let schema = CatalogDefinition::from_json(document, None, None)
            .unwrap()
            .validation_schema()
            .unwrap();
        assert_eq!(
            schema["components"]["Card"]["properties"]["weight"],
            json!({"type":"integer","minimum":2})
        );
        assert_eq!(
            schema["components"]["Card"]["allOf"][0],
            json!({"if":{"required":["highlight"]},"then":{"required":["weight"]}})
        );
        assert!(schema["$defs"].get("Mixin").is_none());
        let mut validation = schema.clone();
        validation["$ref"] = json!("#/components/Card");
        let validator = jsonschema::options().offline().build(&validation).unwrap();
        assert!(!validator.is_valid(&json!({"id":"c","component":"Card","highlight":true})));
        assert!(
            validator.is_valid(&json!({"id":"c","component":"Card","highlight":true,"weight":3}))
        );
    }

    #[test]
    fn canonical_schema_guards_cycles_closure_and_preserves_required_leaves() {
        let base = json!({"protocolVersion":"1.0","catalogId":"custom","components":{"Card":{
            "allOf":[{"$ref":"#/$defs/Mixin"}],
            "properties":{"other":{"type":"string"}},"required":["other"],"additionalProperties":false
        }},"$defs":{"Mixin":{"properties":{"weight":{"$ref":"#/$defs/Weight"}},"required":["weight"]},
        "Weight":{"type":"integer","minimum":2,"description":"authored leaf"}}});
        let schema = CatalogDefinition::from_json(base.clone(), None, None)
            .unwrap()
            .validation_schema()
            .unwrap();
        assert_eq!(schema["$defs"]["Weight"], base["$defs"]["Weight"]);
        let mut validation = schema.clone();
        validation["$ref"] = json!("#/components/Card");
        let validator = jsonschema::options().offline().build(&validation).unwrap();
        assert!(validator.is_valid(&json!({"id":"c","component":"Card","weight":3,"other":"x"})));
        for value in [
            json!({"id":"c","component":"Card","other":"x"}),
            json!({"id":"c","component":"Card","weight":3}),
            json!({"id":"c","component":"Card","weight":3,"other":"x","extra":1}),
        ] {
            assert!(!validator.is_valid(&value));
        }
        for key in ["additionalProperties", "unevaluatedProperties"] {
            let mut closed = base.clone();
            closed["$defs"]["Mixin"][key] = json!(false);
            assert!(
                CatalogDefinition::from_json(closed, None, None)
                    .unwrap()
                    .validation_schema()
                    .is_err()
            );
        }
        let mut cycle = base.clone();
        cycle["$defs"]["Mixin"]["allOf"] = json!([{"$ref":"#/$defs/Mixin"}]);
        assert!(
            CatalogDefinition::from_json(cycle, None, None)
                .unwrap()
                .validation_schema()
                .is_err()
        );
        let mut unknown = base.clone();
        unknown["components"]["Card"]["allOf"] = json!([{"$ref":"#/$defs/Missing"}]);
        assert!(CatalogDefinition::from_json(unknown, None, None).is_err());
        let mut deep = json!({});
        for _ in 0..130 {
            deep = json!({"allOf":[deep]});
        }
        let mut bounded = base;
        bounded["components"]["Card"] = deep;
        assert!(CatalogDefinition::from_json(bounded, None, None).is_err());
    }

    #[test]
    fn draft_version_is_not_treated_as_released_current_catalog() {
        let raw = json!({"catalogId":"draft","protocolVersion":"v1.0","components":{"A_ℓ":{"type":"object"}}});
        assert!(CatalogDefinition::from_json(raw.clone(), None, None).is_err());
        assert!(crate::capabilities::parse_inline_catalog(&raw).is_err());
    }

    #[test]
    fn function_arguments_bundle_common_types_and_local_defaults() {
        use crate::catalog::function_api::{AllowedCallers, FunctionImplementation, ReturnType};
        let raw = json!({"catalogId":"custom","protocolVersion":"1.0","functions":{"quote":{
            "returnType":"string","properties":{"args":{"type":"object","properties":{
                "label":{"$ref":"common_types.json#/$defs/DynamicString"},
                "options":{"$ref":"#/$defs/Options"}
            },"required":["label"],"additionalProperties":false}}
        }},"$defs":{"Options":{"type":"object","default":{},"properties":{"count":{"type":"integer","default":2}}}}});
        let definition = CatalogDefinition::from_json(raw, None, None).unwrap();
        let schema = definition.function_args_schema("quote").unwrap();
        let function = crate::catalog::SchemaOnlyFunction::new("quote".into(), ReturnType::String)
            .with_policy(schema, AllowedCallers::RendererOnly, false)
            .unwrap();
        let mut args = std::collections::HashMap::from([("label".into(), json!("Hello"))]);
        function.prepare_args(&mut args).unwrap();
        assert_eq!(args["options"]["count"], 2);
        args.insert("label".into(), json!(42));
        assert!(function.prepare_args(&mut args).is_err());
        assert!(definition.function_args_schema("hidden").is_err());
    }

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
