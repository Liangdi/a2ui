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
        self.args_validator = Some(jsonschema::validator_for(&args).map_err(|error| {
            A2uiError::Validation(format!("invalid function args schema: {error}"))
        })?);
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
                A2uiError::Validation(format!("invalid function arguments: {error}"))
            })?;
        }
        Ok(())
    }

    fn execute(
        &self,
        _args: &HashMap<String, serde_json::Value>,
        _context: &DataContext,
    ) -> Result<serde_json::Value, A2uiError> {
        Err(A2uiError::NoNativeImplementation(self.name.to_string()))
    }
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
