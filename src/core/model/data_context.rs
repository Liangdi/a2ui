//! Scoped data access with dynamic value resolution.

use std::collections::HashMap;

use serde_json::Value;

use super::data_model::DataModel;
use crate::core::protocol::common_types::{
    DynamicBoolean, DynamicBooleanCondition, DynamicNumber, DynamicString,
    DynamicValue, FunctionCall,
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
    functions: &'a HashMap<String, Box<dyn crate::core::catalog::function_api::FunctionImplementation>>,
}

impl<'a> DataContext<'a> {
    /// Create a new DataContext at the root scope.
    pub fn new(
        data_model: &'a DataModel,
        functions: &'a HashMap<String, Box<dyn crate::core::catalog::function_api::FunctionImplementation>>,
    ) -> Self {
        Self {
            data_model,
            base_path: String::new(),
            functions,
        }
    }

    /// Create a nested context for template iteration.
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
        }
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
