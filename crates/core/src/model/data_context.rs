//! Scoped data access with dynamic value resolution.

use std::collections::HashMap;

use serde_json::Value;

use super::data_model::DataModel;
use crate::protocol::common_types::{
    DynamicBoolean, DynamicBooleanCondition, DynamicNumber, DynamicString, DynamicValue,
    FunctionCall,
};

/// A scoped window into the DataModel used during rendering.
///
/// Handles:
/// - Absolute paths (starting with `/`)
/// - Relative paths (no leading `/`, resolved against `base_path`)
/// - Dynamic value resolution (literals, bindings, function calls)
#[derive(Clone)]
pub struct DataContext<'a> {
    data_model: &'a DataModel,
    base_path: String,
    functions: &'a HashMap<String, Box<dyn crate::catalog::function_api::FunctionImplementation>>,
    /// The 0-based index of the current item when this context was created for
    /// a `ChildList::Template` iteration, or `None` outside a template list.
    ///
    /// Drives the `@index` system function (see
    /// [`resolve_index`](Self::resolve_index)). Set by
    /// [`ComponentContext::new`](crate::model::component_context::ComponentContext::new)
    /// from the item's base path, and overridable via
    /// [`with_template_index`](Self::with_template_index).
    template_index: Option<usize>,
    catalog_id: Option<&'a str>,
    agent_caller: bool,
    expression_errors: Option<&'a std::cell::RefCell<Vec<crate::error::A2uiError>>>,
    catalogs: Option<&'a HashMap<String, crate::catalog::Catalog>>,
    catalog_versions: Option<&'a HashMap<String, String>>,
    surface_catalog_id: Option<&'a str>,
    current_protocol: bool,
    user_activated: bool,
}

impl<'a> DataContext<'a> {
    /// Create a new DataContext at the root scope.
    pub fn new(
        data_model: &'a DataModel,
        functions: &'a HashMap<
            String,
            Box<dyn crate::catalog::function_api::FunctionImplementation>,
        >,
    ) -> Self {
        Self {
            data_model,
            base_path: String::new(),
            functions,
            template_index: None,
            catalog_id: None,
            agent_caller: false,
            expression_errors: None,
            catalogs: None,
            catalog_versions: None,
            surface_catalog_id: None,
            current_protocol: false,
            user_activated: false,
        }
    }

    /// Create a nested context for template iteration.
    ///
    /// The nested context starts with no template index; callers that need the
    /// `@index` system function (e.g. when expanding a `ChildList::Template`)
    /// set it via [`with_template_index`](Self::with_template_index).
    pub fn nested(&self, relative_path: &str) -> DataContext<'a> {
        let new_base = if self.base_path.is_empty() {
            format!("/{}", relative_path)
        } else {
            format!("{}/{}", self.base_path, relative_path)
        };
        DataContext {
            data_model: self.data_model,
            base_path: new_base,
            functions: self.functions,
            template_index: None,
            catalog_id: self.catalog_id,
            agent_caller: self.agent_caller,
            expression_errors: self.expression_errors,
            catalogs: self.catalogs,
            catalog_versions: self.catalog_versions,
            surface_catalog_id: self.surface_catalog_id,
            current_protocol: self.current_protocol,
            user_activated: self.user_activated,
        }
    }

    /// Evaluate functions using released A2UI 1.0 semantics rather than draft behavior.
    pub fn with_current_protocol(mut self) -> Self {
        self.current_protocol = true;
        self
    }

    /// Mark evaluation as originating from an actual host user gesture.
    /// Never use this for incoming messages or passive rendering/checks.
    pub fn with_user_activation(mut self) -> Self {
        self.user_activated = true;
        self
    }

    /// Whether this context uses the released A2UI protocol.
    pub fn is_current_protocol(&self) -> bool {
        self.current_protocol
    }

    /// Enable surface-aware catalog lookup without arbitrary name fallback.
    /// Each ordinary nested call independently uses the surface default. Template
    /// calls made inside a native function retain that function's selected map.
    pub fn with_catalogs(
        mut self,
        catalogs: &'a HashMap<String, crate::catalog::Catalog>,
        default_catalog: Option<&'a str>,
    ) -> Self {
        self.catalogs = Some(catalogs);
        self.surface_catalog_id = default_catalog.filter(|id| !id.is_empty());
        self
    }

    /// Declared catalog protocol versions, used to exclude incompatible catalogs.
    /// Omitted versions retain compatibility with existing native registrations.
    pub fn with_catalog_versions(mut self, versions: &'a HashMap<String, String>) -> Self {
        self.catalog_versions = Some(versions);
        self
    }

    /// Opt-in reactive-template diagnostics. With this sink, failed template
    /// parts contribute empty text and report errors; ordinary evaluation remains strict.
    pub fn with_expression_errors(
        mut self,
        errors: &'a std::cell::RefCell<Vec<crate::error::A2uiError>>,
    ) -> Self {
        self.expression_errors = Some(errors);
        self
    }

    /// Report a template-part failure, or return it when strict evaluation is required.
    pub(crate) fn report_expression_error(
        &self,
        error: crate::error::A2uiError,
    ) -> Result<(), crate::error::A2uiError> {
        if let Some(errors) = self.expression_errors {
            errors.borrow_mut().push(error);
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Identify the catalog whose functions are available to this context.
    /// Explicit overrides to another catalog fail rather than running a same-named function.
    pub fn with_catalog_id(mut self, catalog_id: &'a str) -> Self {
        self.catalog_id = Some(catalog_id);
        self
    }

    /// Apply remote-agent caller restrictions to every nested function evaluation.
    pub fn with_agent_caller(mut self) -> Self {
        self.agent_caller = true;
        self
    }

    fn check_caller(
        &self,
        function: &dyn crate::catalog::function_api::FunctionImplementation,
    ) -> Result<(), crate::error::A2uiError> {
        use crate::catalog::function_api::AllowedCallers;
        // Incoming agent messages cannot acquire trusted user activation.
        let forbidden = (function.requires_user_activation()
            && (!self.user_activated || self.agent_caller))
            || if self.agent_caller {
                function.allowed_callers() == AllowedCallers::RendererOnly
            } else {
                function.allowed_callers() == AllowedCallers::AgentOnly
            };
        if forbidden {
            return Err(crate::error::A2uiError::Validation(format!(
                "function cannot be called by {}: {}",
                if self.agent_caller {
                    "agent"
                } else {
                    "renderer"
                },
                function.name()
            )));
        }
        Ok(())
    }

    /// Return the template index for this context, if any.
    pub fn template_index(&self) -> Option<usize> {
        self.template_index
    }

    /// Set the template index (builder style). Returns `self` for chaining.
    ///
    /// `Some(i)` enables the `@index` system function for this context;
    /// `None` disables it.
    pub fn with_template_index(mut self, index: Option<usize>) -> Self {
        self.template_index = index;
        self
    }

    /// Set the template index in place.
    pub fn set_template_index(&mut self, index: Option<usize>) {
        self.template_index = index;
    }

    /// Resolve the `@index` system function against this context.
    ///
    /// Returns `None` when not inside a template iteration (the spec defines
    /// `@index` as valid only within a list context, so callers treat `None` as
    /// "no value"). The optional `offset` argument is a `DynamicNumber`
    /// (literal / binding / function call) added to the 0-based index.
    fn resolve_index(&self, args: &HashMap<String, Value>) -> Option<Value> {
        let index = self.template_index?;
        let offset = match args.get("offset") {
            Some(value) => self.resolve_dynamic_json(value).ok()?.as_f64()?,
            None => 0.0,
        };
        Some(serde_json::json!(index as f64 + offset))
    }

    /// Get the current base path.
    pub fn base_path(&self) -> &str {
        &self.base_path
    }

    /// Resolve a possibly-relative pointer to an absolute JSON Pointer.
    pub fn resolve_pointer(&self, path: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else if path.is_empty() {
            self.base_path.clone()
        } else if self.base_path.is_empty() {
            format!("/{}", path)
        } else {
            format!("{}/{}", self.base_path, path)
        }
    }

    /// Get a value at a (possibly relative) path.
    pub fn get(&self, path: &str) -> Option<Value> {
        let pointer = self.resolve_pointer(path);
        self.data_model.get(&pointer).cloned()
    }

    /// Resolve a DynamicString to its current string value.
    pub fn resolve_dynamic_string(&self, ds: &DynamicString) -> String {
        match ds {
            DynamicString::Literal(s) => s.clone(),
            DynamicString::Binding(b) => self.resolve_binding_to_string(&b.path),
            DynamicString::Function(fc) => {
                let result = self.execute_function(fc);
                value_to_string(&result)
            }
        }
    }

    /// Resolve a DynamicNumber.
    pub fn resolve_dynamic_number(&self, dn: &DynamicNumber) -> f64 {
        match dn {
            DynamicNumber::Literal(n) => *n,
            DynamicNumber::Binding(b) => self
                .resolve_binding(&b.path)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            DynamicNumber::Function(fc) => {
                let result = self.execute_function(fc);
                result.as_f64().unwrap_or(0.0)
            }
        }
    }

    /// Resolve a DynamicBoolean.
    pub fn resolve_dynamic_boolean(&self, db: &DynamicBoolean) -> bool {
        match db {
            DynamicBoolean::Literal(b) => *b,
            DynamicBoolean::Binding(b) => self
                .resolve_binding(&b.path)
                .map(|v| value_to_boolean(&v))
                .unwrap_or(false),
            DynamicBoolean::Function(fc) => {
                let result = self.execute_function(fc);
                value_to_boolean(&result)
            }
        }
    }

    /// Resolve a DynamicBooleanCondition (same logic as DynamicBoolean).
    pub fn resolve_dynamic_boolean_condition(&self, db: &DynamicBooleanCondition) -> bool {
        match db {
            DynamicBooleanCondition::Literal(b) => *b,
            DynamicBooleanCondition::Binding(b) => self
                .resolve_binding(&b.path)
                .map(|v| value_to_boolean(&v))
                .unwrap_or(false),
            DynamicBooleanCondition::Function(fc) => {
                let result = self.execute_function(fc);
                value_to_boolean(&result)
            }
        }
    }

    /// Resolve a validation condition without discarding its structured diagnostics.
    /// Legacy literal booleans are retained, while current wire conditions use a
    /// binding or function. A failed result receives the rule's fallback message
    /// and default error severity only when those fields were not supplied.
    pub fn resolve_check_rule(
        &self,
        rule: &crate::protocol::common_types::CheckRule,
    ) -> Result<crate::protocol::common_types::ValidationResult, crate::error::A2uiError> {
        use crate::protocol::common_types::{ValidationResult, ValidationSeverity};
        let value = match &rule.condition {
            DynamicBooleanCondition::Literal(value) => Value::Bool(*value),
            DynamicBooleanCondition::Binding(binding) => {
                self.resolve_binding(&binding.path).unwrap_or(Value::Null)
            }
            DynamicBooleanCondition::Function(call) => self.try_execute_function(call)?,
        };
        let mut result = if value.get("valid").and_then(Value::as_bool).is_some() {
            serde_json::from_value::<ValidationResult>(value)?
        } else {
            ValidationResult {
                valid: value_to_boolean(&value),
                code: None,
                message: None,
                severity: None,
            }
        };
        if !result.valid {
            if result.message.is_none() && !rule.message.is_empty() {
                result.message = Some(rule.message.clone());
            }
            result.severity.get_or_insert(ValidationSeverity::Error);
        }
        Ok(result)
    }

    /// Resolve a DynamicValue to a serde_json::Value.
    pub fn resolve_dynamic_value(&self, dv: &DynamicValue) -> Value {
        match dv {
            DynamicValue::String(s) => Value::String(s.clone()),
            DynamicValue::Number(n) => serde_json::json!(*n),
            DynamicValue::Boolean(b) => Value::Bool(*b),
            DynamicValue::Array(arr) => Value::Array(
                arr.iter()
                    .map(|value| self.resolve_arg_value(value))
                    .collect(),
            ),
            DynamicValue::Object(object) => Value::Object(
                object
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.strip_prefix("@@")
                                .map(|suffix| format!("@{suffix}"))
                                .unwrap_or_else(|| key.clone()),
                            self.resolve_arg_value(value),
                        )
                    })
                    .collect(),
            ),
            DynamicValue::Null => Value::Null,
            DynamicValue::Binding(b) => self.resolve_binding(&b.path).unwrap_or(Value::Null),
            DynamicValue::Function(fc) => self.execute_function(fc),
        }
    }

    /// Resolve a binding path to a Value.
    fn resolve_binding(&self, path: &str) -> Option<Value> {
        let pointer = self.resolve_pointer(path);
        self.data_model.get(&pointer).cloned()
    }

    /// Resolve a binding to a string (type coercion).
    fn resolve_binding_to_string(&self, path: &str) -> String {
        self.resolve_binding(path)
            .map(|v| value_to_string(&v))
            .unwrap_or_default()
    }

    /// Call a function by name with pre-resolved arguments.
    ///
    /// Returns `None` if the function is not found or execution fails.
    pub fn call_function_by_name(
        &self,
        name: &str,
        args: &HashMap<String, Value>,
    ) -> Option<Value> {
        // System function: @index — only meaningful inside a template iteration.
        if name == "@index" {
            return self.resolve_index(args);
        }
        let func = self.functions.get(name)?;
        self.check_caller(func.as_ref()).ok()?;
        // Resolve each argument value (may contain bindings or nested calls).
        let mut resolved_args = HashMap::new();
        for (key, val) in args {
            let resolved = self.resolve_dynamic_json(val).ok()?;
            resolved_args.insert(key.clone(), resolved);
        }
        func.execute(&resolved_args, self).ok()
    }

    /// Strict template invocation in the currently executing function's catalog.
    /// Template syntax cannot override catalog identity.
    pub fn try_call_function_by_name(
        &self,
        name: &str,
        args: &HashMap<String, Value>,
    ) -> Result<Value, crate::error::A2uiError> {
        use crate::error::A2uiError;
        if name == "@index" {
            return self.resolve_index(args).ok_or_else(|| A2uiError::Validation("@index function can only be evaluated inside a collection template iteration scope.".into()));
        }
        let function = self
            .functions
            .get(name)
            .ok_or_else(|| A2uiError::InvalidFunctionCall(format!("unknown function: {name}")))?;
        self.check_caller(function.as_ref())?;
        function.validate_args(args)?;
        function.execute(args, self)
    }

    /// Execute a function call, retaining the historical null-on-error display API.
    fn execute_function(&self, fc: &FunctionCall) -> Value {
        self.try_execute_function(fc).unwrap_or(Value::Null)
    }

    pub fn try_execute_function(
        &self,
        fc: &FunctionCall,
    ) -> Result<Value, crate::error::A2uiError> {
        use crate::error::A2uiError;
        if fc.call == "@index" {
            return self.resolve_index(&fc.args).ok_or_else(|| A2uiError::Validation(
                "@index function can only be evaluated inside a collection template iteration scope.".into()));
        }
        let mut execution_context = self.clone();
        if let Some(catalogs) = self.catalogs {
            let selected = if self.current_protocol {
                fc.catalog_id.as_deref().or(self.surface_catalog_id)
            } else {
                self.surface_catalog_id
            };
            let selected = selected
                .ok_or_else(|| A2uiError::Validation("surface has no default catalogId".into()))?;
            let (id, catalog) = catalogs
                .get_key_value(selected)
                .ok_or_else(|| A2uiError::Validation(format!("Catalog not found: {selected}")))?;
            if let Some(version) = self.catalog_versions.and_then(|versions| versions.get(id)) {
                let compatible = if self.current_protocol {
                    version == "1.0"
                } else {
                    version != "1.0"
                };
                if !compatible {
                    return Err(A2uiError::Validation(format!(
                        "Catalog not found: {selected} (incompatible protocol version)"
                    )));
                }
            }
            execution_context.functions = &catalog.functions;
            execution_context.catalog_id = Some(id);
        } else if let Some(catalog) = fc.catalog_id.as_deref()
            && self.catalog_id != Some(catalog)
        {
            return Err(A2uiError::Validation(format!(
                "function catalog is unavailable in this context: {catalog}"
            )));
        }
        let func = execution_context
            .functions
            .get(&fc.call)
            .ok_or_else(|| A2uiError::Validation(format!("unknown function: {}", fc.call)))?;
        self.check_caller(func.as_ref())?;
        let mut args = HashMap::new();
        for (key, value) in &fc.args {
            args.insert(key.clone(), self.resolve_dynamic_json(value)?);
        }
        func.execute(&args, &execution_context)
    }

    /// Evaluate a v1.0 dynamic JSON value recursively without swallowing errors.
    /// Literal dictionaries containing `path` or `call` remain dictionaries.
    /// An escaped key is unescaped once; its result is not evaluated a second time.
    pub fn resolve_dynamic_json(&self, value: &Value) -> Result<Value, crate::error::A2uiError> {
        // Preserve integer precision and the JSON number representation. The
        // legacy typed Number variant uses f64, but raw v1.0 JSON need not.
        if !value.is_object() && !value.is_array() {
            return Ok(value.clone());
        }
        let dynamic: DynamicValue = serde_json::from_value(value.clone())?;
        match dynamic {
            DynamicValue::Array(values) => values
                .iter()
                .map(|value| self.resolve_dynamic_json(value))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            DynamicValue::Object(object) => object
                .iter()
                .map(|(key, value)| {
                    let key = key
                        .strip_prefix("@@")
                        .map(|suffix| format!("@{suffix}"))
                        .unwrap_or_else(|| key.clone());
                    self.resolve_dynamic_json(value).map(|value| (key, value))
                })
                .collect::<Result<serde_json::Map<_, _>, _>>()
                .map(Value::Object),
            DynamicValue::Function(call) => self.try_execute_function(&call),
            other => Ok(self.resolve_dynamic_value(&other)),
        }
    }

    fn resolve_arg_value(&self, value: &Value) -> Value {
        self.resolve_dynamic_json(value).unwrap_or(Value::Null)
    }
}

/// A2UI v1.0 DynamicBoolean coercion, including structured validation results.
pub fn value_to_boolean(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => value.eq_ignore_ascii_case("true"),
        Value::Object(value) => value.get("valid").and_then(Value::as_bool).unwrap_or(false),
        _ => false,
    }
}

/// Convert a serde_json::Value to a display string per A2UI type coercion rules.
pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => v.to_string(),
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use crate::model::data_model::DataModel;
    use crate::protocol::common_types::{DynamicNumber, FunctionCall};
    use serde_json::json;

    /// Build a 'static DataContext with the given template index and an empty
    /// function table. (DataModel + HashMap are leaked — test-only.)
    fn ctx(idx: Option<usize>) -> DataContext<'static> {
        let dm = Box::leak(Box::new(DataModel::new()));
        let fns = Box::leak(Box::new(HashMap::new()));
        DataContext::new(dm, fns).with_template_index(idx)
    }

    fn index_call(args: &[(&str, Value)]) -> FunctionCall {
        let mut map = HashMap::new();
        for (k, v) in args {
            map.insert((*k).to_string(), v.clone());
        }
        FunctionCall {
            call: "@index".to_string(),
            catalog_id: None,
            args: map,
        }
    }

    #[test]
    fn at_index_without_template_context_is_null() {
        // Outside a template list the spec leaves @index undefined; the data
        // context degrades to null → 0.0 for a numeric context.
        let ctx = ctx(None);
        let dn = DynamicNumber::Function(index_call(&[]));
        assert_eq!(ctx.resolve_dynamic_number(&dn), 0.0);
        // call_function_by_name returns None (no template context).
        assert_eq!(ctx.call_function_by_name("@index", &HashMap::new()), None);
    }

    #[test]
    fn at_index_returns_zero_based_index() {
        let ctx = ctx(Some(2));
        let dn = DynamicNumber::Function(index_call(&[]));
        assert_eq!(ctx.resolve_dynamic_number(&dn), 2.0);
    }

    #[test]
    fn at_index_with_literal_offset() {
        let ctx = ctx(Some(2));
        let dn = DynamicNumber::Function(index_call(&[("offset", json!(1))]));
        assert_eq!(ctx.resolve_dynamic_number(&dn), 3.0);
    }

    #[test]
    fn at_index_with_bound_offset() {
        let dm = Box::leak(Box::new(DataModel::from_value(json!({"step": 10}))));
        let fns = Box::leak(Box::new(HashMap::new()));
        let ctx = DataContext::new(dm, fns).with_template_index(Some(0));
        let dn = DynamicNumber::Function(index_call(&[("offset", json!({"@path": "/step"}))]));
        assert_eq!(ctx.resolve_dynamic_number(&dn), 10.0); // 0 + step(10)
    }

    #[test]
    fn at_index_via_call_function_by_name() {
        let ctx = ctx(Some(4));
        let mut args = HashMap::new();
        args.insert("offset".to_string(), json!(1));
        assert_eq!(ctx.call_function_by_name("@index", &args), Some(json!(5.0)));
    }

    #[test]
    fn at_index_zero_plus_offset() {
        // First item with a 1-based offset → "1".
        let ctx = ctx(Some(0));
        let dn = DynamicNumber::Function(index_call(&[("offset", json!(1))]));
        assert_eq!(ctx.resolve_dynamic_number(&dn), 1.0);
    }
}

#[cfg(test)]
mod v1_dynamic_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn evaluates_recursive_bindings_and_preserves_literal_directive_names() {
        let dm = DataModel::from_value(json!({"name": "Ada"}));
        let functions = HashMap::new();
        let context = DataContext::new(&dm, &functions);
        let input = json!({
            "list": [{"@path": "/name"}, null],
            "path": "/ordinary", "call": "ordinary",
            "@@path": "literal", "nested": {"@@@key": true}
        });
        assert_eq!(
            context.resolve_dynamic_json(&input).unwrap(),
            json!({
                "list": ["Ada", null], "path": "/ordinary", "call": "ordinary",
                "@path": "literal", "nested": {"@@key": true}
            })
        );
    }

    #[test]
    fn raw_values_preserve_large_integer_precision() {
        let dm = DataModel::new();
        let functions = HashMap::new();
        let context = DataContext::new(&dm, &functions);
        let value = json!({"integer": 9_007_199_254_740_993_u64});
        assert_eq!(context.resolve_dynamic_json(&value).unwrap(), value);
    }

    #[test]
    fn rejects_reserved_or_ambiguous_directives_at_any_depth() {
        let dm = DataModel::new();
        let functions = HashMap::new();
        let context = DataContext::new(&dm, &functions);
        for input in [
            json!({"@unknown": true}),
            json!([{"@path": "/x", "extra": 1}]),
            json!({"nested": {"@path": "/x", "@call": "f"}}),
            json!({"@call": "f", "args": {"x": {"@unknown": 1}}}),
        ] {
            assert!(context.resolve_dynamic_json(&input).is_err(), "{input}");
        }
    }

    #[test]
    fn modern_index_requires_template_context_and_resolves_offset() {
        let dm = DataModel::from_value(json!({"offset": 3}));
        let functions = HashMap::new();
        let context = DataContext::new(&dm, &functions);
        let call = json!({"@call": "@index", "args": {"offset": {"@path": "/offset"}}});
        assert!(context.resolve_dynamic_json(&call).is_err());
        assert_eq!(
            context
                .with_template_index(Some(2))
                .resolve_dynamic_json(&call)
                .unwrap(),
            json!(5.0)
        );
    }

    #[test]
    fn rejects_unavailable_function_catalog() {
        let dm = DataModel::new();
        let functions = HashMap::new();
        let context = DataContext::new(&dm, &functions).with_catalog_id("expected");
        assert!(
            context
                .resolve_dynamic_json(&json!({"@call": "f", "catalogId": "other"}))
                .is_err()
        );
    }

    #[test]
    fn boolean_coercion_matches_v1_table() {
        for value in [
            json!(true),
            json!("TrUe"),
            json!(1),
            json!(-1),
            json!({"valid": true}),
        ] {
            assert!(value_to_boolean(&value), "{value}");
        }
        for value in [
            json!(false),
            json!("FALSE"),
            json!("hello"),
            json!(0),
            json!(null),
            json!([]),
            json!({}),
            json!({"valid": false}),
        ] {
            assert!(!value_to_boolean(&value), "{value}");
        }
        let dm = DataModel::from_value(json!({"result": {"valid": true}}));
        let functions = HashMap::new();
        let context = DataContext::new(&dm, &functions);
        let binding: DynamicBoolean = serde_json::from_value(json!({"@path": "/result"})).unwrap();
        assert!(context.resolve_dynamic_boolean(&binding));
    }
}

#[cfg(test)]
mod validation_result_tests {
    use super::*;
    use crate::protocol::common_types::{CheckRule, ValidationSeverity};
    use serde_json::json;

    #[test]
    fn structured_checks_preserve_diagnostics_and_fill_fallbacks() {
        let dm = DataModel::from_value(
            json!({"error": {"valid": false, "code": "BAD", "message": "specific", "severity": "warning"}, "empty": {"valid": false}}),
        );
        let functions = HashMap::new();
        let context = DataContext::new(&dm, &functions);
        let rule: CheckRule = serde_json::from_value(
            json!({"condition": {"@path": "/error"}, "message": "fallback"}),
        )
        .unwrap();
        let result = context.resolve_check_rule(&rule).unwrap();
        assert_eq!(result.code.as_deref(), Some("BAD"));
        assert_eq!(result.message.as_deref(), Some("specific"));
        assert_eq!(result.severity, Some(ValidationSeverity::Warning));
        let rule: CheckRule = serde_json::from_value(
            json!({"condition": {"@path": "/empty"}, "message": "fallback"}),
        )
        .unwrap();
        let result = context.resolve_check_rule(&rule).unwrap();
        assert_eq!(result.message.as_deref(), Some("fallback"));
        assert_eq!(result.severity, Some(ValidationSeverity::Error));
    }
}

#[cfg(test)]
mod caller_tests {
    use super::*;
    use crate::catalog::function_api::{AllowedCallers, FunctionImplementation, ReturnType};
    use crate::error::A2uiError;
    use serde_json::json;

    struct Function {
        name: &'static str,
        allowed: AllowedCallers,
        activation: bool,
    }
    impl FunctionImplementation for Function {
        fn name(&self) -> &'static str {
            self.name
        }
        fn return_type(&self) -> ReturnType {
            ReturnType::Any
        }
        fn allowed_callers(&self) -> AllowedCallers {
            self.allowed
        }
        fn requires_user_activation(&self) -> bool {
            self.activation
        }
        fn execute(
            &self,
            args: &HashMap<String, Value>,
            _: &DataContext,
        ) -> Result<Value, A2uiError> {
            Ok(json!(args))
        }
    }

    #[test]
    fn caller_restrictions_apply_inside_nested_arguments() {
        let dm = DataModel::new();
        let mut functions: HashMap<String, Box<dyn FunctionImplementation>> = HashMap::new();
        for (name, allowed, activation) in [
            ("outer", AllowedCallers::RendererOrAgent, false),
            ("local", AllowedCallers::RendererOnly, false),
            ("remote", AllowedCallers::AgentOnly, false),
            ("gesture", AllowedCallers::RendererOrAgent, true),
        ] {
            functions.insert(
                name.into(),
                Box::new(Function {
                    name,
                    allowed,
                    activation,
                }),
            );
        }
        let renderer = DataContext::new(&dm, &functions);
        assert!(
            renderer
                .resolve_dynamic_json(&json!({"@call": "remote"}))
                .is_err()
        );
        let agent = DataContext::new(&dm, &functions).with_agent_caller();
        for name in ["local", "gesture"] {
            let nested = json!({"@call": "outer", "args": {"value": [{"@call": name}]}});
            assert!(agent.resolve_dynamic_json(&nested).is_err());
            assert!(
                agent
                    .nested("items/0")
                    .resolve_dynamic_json(&nested)
                    .is_err()
            );
        }
        assert!(
            agent
                .resolve_dynamic_json(&json!({"@call": "remote"}))
                .is_ok()
        );
    }
}
