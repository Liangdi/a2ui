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
        }
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
        self.template_index.map(|idx| {
            let offset = args
                .get("offset")
                .map(|v| self.resolve_arg_value(v).as_f64().unwrap_or(0.0))
                .unwrap_or(0.0);
            serde_json::json!(idx as f64 + offset)
        })
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
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            DynamicBoolean::Function(fc) => {
                let result = self.execute_function(fc);
                result.as_bool().unwrap_or(false)
            }
        }
    }

    /// Resolve a DynamicBooleanCondition (same logic as DynamicBoolean).
    pub fn resolve_dynamic_boolean_condition(&self, db: &DynamicBooleanCondition) -> bool {
        match db {
            DynamicBooleanCondition::Literal(b) => *b,
            DynamicBooleanCondition::Binding(b) => self
                .resolve_binding(&b.path)
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            DynamicBooleanCondition::Function(fc) => {
                let result = self.execute_function(fc);
                result.as_bool().unwrap_or(false)
            }
        }
    }

    /// Resolve a DynamicValue to a serde_json::Value.
    pub fn resolve_dynamic_value(&self, dv: &DynamicValue) -> Value {
        match dv {
            DynamicValue::String(s) => Value::String(s.clone()),
            DynamicValue::Number(n) => serde_json::json!(*n),
            DynamicValue::Boolean(b) => Value::Bool(*b),
            DynamicValue::Array(arr) => Value::Array(arr.clone()),
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
        // Resolve each argument value (may contain bindings or nested calls).
        let mut resolved_args = HashMap::new();
        for (key, val) in args {
            let resolved = self.resolve_arg_value(val);
            resolved_args.insert(key.clone(), resolved);
        }
        func.execute(&resolved_args, self).ok()
    }

    /// Execute a function call.
    fn execute_function(&self, fc: &FunctionCall) -> Value {
        // System function: @index — only valid inside a template iteration.
        if fc.call == "@index" {
            return self.resolve_index(&fc.args).unwrap_or(Value::Null);
        }
        let Some(func) = self.functions.get(&fc.call) else {
            return Value::Null;
        };

        // Resolve each argument (they can contain DynamicValues)
        let mut resolved_args = HashMap::new();
        for (key, val) in &fc.args {
            // Try to resolve as a DynamicValue if it's an object with "path" or "call"
            let resolved = self.resolve_arg_value(val);
            resolved_args.insert(key.clone(), resolved);
        }

        match func.execute(&resolved_args, self) {
            Ok(v) => v,
            Err(_) => Value::Null,
        }
    }

    /// Resolve an argument value that might be a DynamicValue.
    fn resolve_arg_value(&self, val: &Value) -> Value {
        if let Some(obj) = val.as_object() {
            if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
                return self.resolve_binding(path).unwrap_or(Value::Null);
            }
            if let Some(call) = obj.get("call").and_then(|v| v.as_str()) {
                let args = obj
                    .get("args")
                    .and_then(|v| v.as_object())
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| (k.clone(), self.resolve_arg_value(v)))
                            .collect::<HashMap<_, _>>()
                    })
                    .unwrap_or_default();
                let fc = FunctionCall {
                    call: call.to_string(),
                    args,
                };
                return self.execute_function(&fc);
            }
        }
        val.clone()
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
        let dn = DynamicNumber::Function(index_call(&[("offset", json!({"path": "/step"}))]));
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
