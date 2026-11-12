//! Schema-only function placeholder for inline catalogs.
//!
//! Functions declared in an inline catalog have a schema (argument shape and
//! return type) but no native Rust implementation. To let the existing
//! `handle_call_function` machinery discover and uniformly reject calls to
//! such functions, we register a [`SchemaOnlyFunction`] in the catalog's
//! function map. Its [`execute`](FunctionImplementation::execute) always errors.

use std::collections::HashMap;

use super::function_api::{AllowedCallers, FunctionImplementation, ReturnType};
use crate::error::A2uiError;
use crate::model::data_context::DataContext;

/// A function that carries a schema but has no native implementation.
///
/// This is used to represent functions declared in *inline catalogs* that the
/// server sends as part of capabilities negotiation. The client knows the
/// function exists (and its declared return type) but cannot execute it.
pub struct SchemaOnlyFunction {
    name: &'static str,
    return_type: ReturnType,
    callers: AllowedCallers,
    user_activation: bool,
    args_validator: Option<jsonschema::Validator>,
    args_schema: Option<serde_json::Value>,
}

impl SchemaOnlyFunction {
    /// Create a new schema-only function from a runtime `String` name.
    ///
    /// The name is leaked via `Box::leak` to produce the `&'static str`
    /// required by [`FunctionImplementation::name`]. This leak is **bounded**:
    /// inline catalogs are registered exactly once at startup (via
    /// [`MessageProcessor::register_inline_catalog`](crate::message_processor::MessageProcessor::register_inline_catalog))
    /// and never unloaded for the lifetime of the processor, so the leaked
    /// memory is proportional to the (small, fixed) set of inline functions a
    /// client advertises — it does not grow unboundedly.
    pub fn new(name: String, return_type: ReturnType) -> Self {
        // Bounded leak: inline catalogs are registered once at startup.
        let leaked: &'static str = Box::leak(name.into_boxed_str());
        Self {
            name: leaked,
            return_type,
            callers: AllowedCallers::RendererOnly,
            user_activation: false,
            args_validator: None,
            args_schema: None,
        }
    }
    /// Retain declared caller boundaries and compile a local-only argument schema.
    /// External schema references are disabled; inline declarations grant no native code.
    pub fn with_policy(
        mut self,
        args: serde_json::Value,
        callers: AllowedCallers,
        user_activation: bool,
    ) -> Result<Self, A2uiError> {
        self.args_validator = Some(jsonschema::options().offline().build(&args).map_err(
            |error| A2uiError::Validation(format!("invalid function args schema: {error}")),
        )?);
        self.args_schema = Some(args);
        self.callers = callers;
        self.user_activation = user_activation;
        Ok(self)
    }
}

impl FunctionImplementation for SchemaOnlyFunction {
    fn name(&self) -> &'static str {
        self.name
    }

    fn return_type(&self) -> ReturnType {
        self.return_type
    }

    fn is_native(&self) -> bool {
        false
    }

    fn allowed_callers(&self) -> AllowedCallers {
        self.callers
    }

    fn requires_user_activation(&self) -> bool {
        self.user_activation
    }

    fn validate_args(&self, args: &HashMap<String, serde_json::Value>) -> Result<(), A2uiError> {
        if let Some(validator) = &self.args_validator {
            let value = serde_json::to_value(args)?;
            validator.validate(&value).map_err(|error| {
                A2uiError::Validation(format!(
                    "Validation failed for function '{}': {error}",
                    self.name
                ))
            })?;
        }
        Ok(())
    }

    fn prepare_args(&self, args: &mut HashMap<String, serde_json::Value>) -> Result<(), A2uiError> {
        if let Some(schema) = &self.args_schema {
            let mut value = serde_json::to_value(&*args)?;
            apply_defaults(schema, schema, &mut value, 0)?;
            *args = serde_json::from_value(value)?;
        }
        self.validate_args(args)
    }

    fn execute(
        &self,
        _args: &HashMap<String, serde_json::Value>,
        _context: &DataContext,
    ) -> Result<serde_json::Value, A2uiError> {
        Err(A2uiError::NoNativeImplementation(self.name.to_string()))
    }
}

// Defaults apply only to missing properties, never replace explicit null.
fn apply_defaults(
    root: &serde_json::Value,
    schema: &serde_json::Value,
    value: &mut serde_json::Value,
    depth: usize,
) -> Result<(), A2uiError> {
    if depth > 128 {
        return Err(A2uiError::Validation(
            "argument defaults exceed depth limit".into(),
        ));
    }
    if let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        if let Some(pointer) = reference.strip_prefix('#') {
            if let Some(target) = root.pointer(pointer) {
                apply_defaults(root, target, value, depth + 1)?;
            }
        }
    }
    if let Some(parts) = schema.get("allOf").and_then(serde_json::Value::as_array) {
        for part in parts {
            apply_defaults(root, part, value, depth + 1)?;
        }
    }
    if let (Some(properties), Some(object)) = (
        schema
            .get("properties")
            .and_then(serde_json::Value::as_object),
        value.as_object_mut(),
    ) {
        for (name, property) in properties {
            if !object.contains_key(name)
                && let Some(default) = declared_default(root, property, depth + 1)?
            {
                object.insert(name.clone(), default);
            }
            if let Some(child) = object.get_mut(name) {
                apply_defaults(root, property, child, depth + 1)?;
            }
        }
    }
    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array_mut()) {
        for child in array {
            apply_defaults(root, items, child, depth + 1)?;
        }
    }
    Ok(())
}

fn declared_default(
    root: &serde_json::Value,
    schema: &serde_json::Value,
    depth: usize,
) -> Result<Option<serde_json::Value>, A2uiError> {
    if depth > 128 {
        return Err(A2uiError::Validation(
            "argument defaults exceed depth limit".into(),
        ));
    }
    if let Some(default) = schema.get("default") {
        return Ok(Some(default.clone()));
    }
    if let Some(reference) = schema
        .get("$ref")
        .and_then(serde_json::Value::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
    {
        if let Some(target) = root.pointer(reference) {
            return declared_default(root, target, depth + 1);
        }
    }
    if let Some(parts) = schema.get("allOf").and_then(serde_json::Value::as_array) {
        let mut found = None;
        for part in parts {
            if let Some(default) = declared_default(root, part, depth + 1)? {
                if found.as_ref().is_some_and(|prior| prior != &default) {
                    return Err(A2uiError::Validation(
                        "conflicting allOf argument defaults".into(),
                    ));
                }
                found = Some(default);
            }
        }
        return Ok(found);
    }
    Ok(None)
}

/// Parse a return-type string (as found in a catalog's `returnType` field)
/// into the [`ReturnType`] enum. Unknown strings map to [`ReturnType::Any`].
pub fn parse_return_type(s: &str) -> ReturnType {
    match s {
        "string" => ReturnType::String,
        "number" => ReturnType::Number,
        "boolean" => ReturnType::Boolean,
        "array" => ReturnType::Array,
        "object" => ReturnType::Object,
        "void" => ReturnType::Void,
        _ => ReturnType::Any, // "any" and anything unexpected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_resolve_local_refs_and_all_of() {
        let f = SchemaOnlyFunction::new("quote".into(), ReturnType::Any)
            .with_policy(
                serde_json::json!({
                    "type":"object", "$defs":{"Limit":{"type":"integer","default":10}},
                    "properties":{"limit":{"$ref":"#/$defs/Limit"}},"required":["limit"],
                    "allOf":[{"properties":{"currency":{"type":"string","default":"USD"}}}]
                }),
                AllowedCallers::RendererOnly,
                false,
            )
            .unwrap();
        let mut args = HashMap::new();
        f.prepare_args(&mut args).unwrap();
        assert_eq!(args["limit"], 10);
        assert_eq!(args["currency"], "USD");
    }

    #[test]
    fn name_and_return_type_round_trip() {
        let f = SchemaOnlyFunction::new("shout".to_string(), ReturnType::String);
        assert_eq!(f.name(), "shout");
        assert_eq!(f.return_type(), ReturnType::String);
    }

    #[test]
    fn execute_errors() {
        let f = SchemaOnlyFunction::new("noop".to_string(), ReturnType::Void);
        let dm = crate::model::data_model::DataModel::new();
        let empty: HashMap<String, Box<dyn FunctionImplementation>> = HashMap::new();
        let ctx = DataContext::new(&dm, &empty);
        let args = HashMap::new();
        let res = f.execute(&args, &ctx);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.to_string().contains("no native implementation"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_return_type_variants() {
        assert_eq!(parse_return_type("string"), ReturnType::String);
        assert_eq!(parse_return_type("number"), ReturnType::Number);
        assert_eq!(parse_return_type("boolean"), ReturnType::Boolean);
        assert_eq!(parse_return_type("array"), ReturnType::Array);
        assert_eq!(parse_return_type("object"), ReturnType::Object);
        assert_eq!(parse_return_type("void"), ReturnType::Void);
        assert_eq!(parse_return_type("any"), ReturnType::Any);
        assert_eq!(parse_return_type("bogus"), ReturnType::Any);
    }
}
