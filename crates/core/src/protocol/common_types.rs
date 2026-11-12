//! A2UI v1.0 Common Types
//!
//! Mirrors the JSON Schema `common_types.json` — the core data binding types
//! used throughout the A2UI protocol.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Component references
// ---------------------------------------------------------------------------

/// Unique identifier for a component instance within a surface.
pub type ComponentId = String;

// ---------------------------------------------------------------------------
// Data binding
// ---------------------------------------------------------------------------

/// A JSON Pointer path into the data model.
/// Serialized as `{ "@path": "/some/pointer" }` (legacy typed input also accepts `path`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DataBinding {
    #[serde(rename = "@path", alias = "path")]
    pub path: String,
}

/// A named function call with arguments.
/// Each argument value can itself be any JSON value (including nested Dynamic values).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FunctionCall {
    #[serde(rename = "@call", alias = "call")]
    pub call: String,
    #[serde(default, rename = "catalogId", skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub args: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Dynamic value types — can be a literal, a data-binding, or a function call
// ---------------------------------------------------------------------------

/// A value that is either a literal string, a data-binding, or a function call.
///
/// JSON representations:
/// - Literal: `"Hello"`
/// - Binding: `{ "@path": "/user/name" }`
/// - Function: `{ "@call": "capitalize", "args": { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicString {
    /// A literal string value.
    Literal(String),
    /// A binding to a data model path.
    Binding(DataBinding),
    /// A function call that returns a string.
    Function(FunctionCall),
}

impl DynamicString {
    /// Returns `true` if this is a literal string value.
    pub fn is_literal(&self) -> bool {
        matches!(self, Self::Literal(_))
    }

    /// Returns the literal value if this is a literal, otherwise `None`.
    pub fn as_literal(&self) -> Option<&str> {
        match self {
            Self::Literal(s) => Some(s),
            _ => None,
        }
    }
}

impl Default for DynamicString {
    fn default() -> Self {
        Self::Literal(String::new())
    }
}

impl From<String> for DynamicString {
    fn from(s: String) -> Self {
        Self::Literal(s)
    }
}

impl From<&str> for DynamicString {
    fn from(s: &str) -> Self {
        Self::Literal(s.to_string())
    }
}

/// A value that is either a literal number, a data-binding, or a function call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicNumber {
    Literal(f64),
    Binding(DataBinding),
    Function(FunctionCall),
}

/// A value that is either a literal boolean, a data-binding, or a function call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicBoolean {
    Literal(bool),
    Binding(DataBinding),
    Function(FunctionCall),
}

/// A value that is either a literal boolean (via `condition` key),
/// a data-binding, or a function call — used in `CheckRule` conditions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicBooleanCondition {
    Literal(bool),
    Binding(DataBinding),
    Function(FunctionCall),
}

/// A v1.0 dynamic value. Only `@path` and `@call` objects are directives.
/// Plain `path` / `call` keys are ordinary data; literal leading `@` keys
/// must be doubled on the wire and are unescaped when evaluated.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<serde_json::Value>),
    Binding(DataBinding),
    Function(FunctionCall),
    Object(serde_json::Map<String, serde_json::Value>),
    Null,
}

impl<'de> Deserialize<'de> for DynamicValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Self::from_json(value).map_err(serde::de::Error::custom)
    }
}

impl DynamicValue {
    fn from_json(value: serde_json::Value) -> Result<Self, String> {
        use serde_json::Value;
        match value {
            Value::Null => Ok(Self::Null),
            Value::Bool(value) => Ok(Self::Boolean(value)),
            Value::Number(value) => value
                .as_f64()
                .map(Self::Number)
                .ok_or_else(|| "number cannot be represented as f64".into()),
            Value::String(value) => Ok(Self::String(value)),
            Value::Array(values) => {
                for value in &values {
                    Self::from_json(value.clone())?;
                }
                Ok(Self::Array(values))
            }
            Value::Object(values) => {
                if values.contains_key("@path") {
                    return serde_json::from_value(Value::Object(values))
                        .map(Self::Binding)
                        .map_err(|error| error.to_string());
                }
                if values.contains_key("@call") {
                    let call: FunctionCall = serde_json::from_value(Value::Object(values))
                        .map_err(|error| error.to_string())?;
                    for value in call.args.values() {
                        Self::from_json(value.clone())?;
                    }
                    return Ok(Self::Function(call));
                }
                for (key, value) in &values {
                    if key.starts_with('@') && !key.starts_with("@@") {
                        return Err(format!("unknown reserved dynamic key: {key}"));
                    }
                    Self::from_json(value.clone())?;
                }
                Ok(Self::Object(values))
            }
        }
    }
}

/// A dynamic value that resolves to a list of strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicStringList {
    Literal(Vec<String>),
    Binding(DataBinding),
    Function(FunctionCall),
}

// ---------------------------------------------------------------------------
// Child list — how containers reference their children
// ---------------------------------------------------------------------------

/// Describes the children of a container component.
///
/// Either a static array of component IDs, or a dynamic template that
/// generates children from a data-bound array.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ChildList {
    /// A fixed list of child component IDs.
    Static(Vec<ComponentId>),
    /// A template that iterates over a data-bound array,
    /// instantiating `component_id` for each item.
    #[serde(rename_all = "camelCase")]
    Template {
        component_id: ComponentId,
        path: String,
    },
}

impl Default for ChildList {
    fn default() -> Self {
        Self::Static(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// An action triggered by user interaction (e.g. button click).
///
/// Either dispatches an event to the server, or calls a local function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Action {
    /// Send an event to the server.
    Event { event: ActionEvent },
    /// Execute a local client-side function.
    FunctionCall {
        #[serde(rename = "functionCall", alias = "function_call")]
        function_call: FunctionCall,
    },
}

/// A server-bound event with optional context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_message: Option<DynamicString>,
    #[serde(default)]
    pub context: HashMap<String, DynamicValue>,
    #[serde(default, alias = "want_response", skip_serializing_if = "is_false")]
    pub want_response: bool,
    #[serde(
        default,
        alias = "response_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_path: Option<String>,
}

fn is_false(v: &bool) -> bool {
    !v
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A validation check with a boolean condition and an error message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckRule {
    pub condition: DynamicBooleanCondition,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

/// Structured result produced by a v1.0 check function or data binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationResult {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<ValidationSeverity>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

// ---------------------------------------------------------------------------
// Accessibility
// ---------------------------------------------------------------------------

/// Accessibility attributes for a component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessibilityAttributes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<DynamicString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<DynamicString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LiveRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<DynamicBoolean>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LiveRegion {
    Off,
    Polite,
    Assertive,
}

// ---------------------------------------------------------------------------
// Alignment / Justify enums
// ---------------------------------------------------------------------------

/// Main-axis alignment (maps to flexbox justify-content).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

/// Cross-axis alignment (maps to flexbox align-items).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn childlist_static_array_deserializes() {
        let json = serde_json::json!(["a", "b", "c"]);
        let cl: ChildList = serde_json::from_value(json).unwrap();
        assert_eq!(
            cl,
            ChildList::Static(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn childlist_template_deserializes_camel_case_component_id() {
        // The spec schema (common_types.json) requires the camelCase key
        // `componentId`. This is the form every sample uses, e.g. the
        // "Incremental List" sample's root Column.
        let json = serde_json::json!({ "path": "/restaurants", "componentId": "restaurant_card" });
        let cl: ChildList = serde_json::from_value(json).unwrap();
        match cl {
            ChildList::Template { component_id, path } => {
                assert_eq!(component_id, "restaurant_card");
                assert_eq!(path, "/restaurants");
            }
            other => panic!("expected Template, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod v1_wire_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn directives_emit_current_keys_and_keep_legacy_typed_input_aliases() {
        let binding: DataBinding = serde_json::from_value(json!({"path": "/name"})).unwrap();
        assert_eq!(
            serde_json::to_value(binding).unwrap(),
            json!({"@path": "/name"})
        );
        let call: FunctionCall =
            serde_json::from_value(json!({"call": "f", "catalogId": "c"})).unwrap();
        assert_eq!(
            serde_json::to_value(call).unwrap(),
            json!({"@call": "f", "catalogId": "c"})
        );
    }

    #[test]
    fn plain_path_and_call_dictionaries_are_literals() {
        for value in [json!({"path": "/x"}), json!({"call": "f", "args": {}})] {
            let dynamic: DynamicValue = serde_json::from_value(value.clone()).unwrap();
            assert!(matches!(dynamic, DynamicValue::Object(_)));
            assert_eq!(serde_json::to_value(dynamic).unwrap(), value);
        }
    }

    #[test]
    fn check_message_is_optional() {
        let check: CheckRule = serde_json::from_value(json!({"condition": true})).unwrap();
        assert!(check.message.is_empty());
    }
}

#[cfg(test)]
mod action_accessibility_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn actions_use_current_camel_case_wire_keys() {
        let action: Action =
            serde_json::from_value(json!({"functionCall": {"@call": "f"}})).unwrap();
        assert_eq!(
            serde_json::to_value(action).unwrap(),
            json!({"functionCall": {"@call": "f"}})
        );
        let event: ActionEvent = serde_json::from_value(json!({"name": "send", "userMessage": {"@path": "/label"}, "wantResponse": true, "responsePath": "/reply"})).unwrap();
        let wire = serde_json::to_value(event).unwrap();
        assert_eq!(wire["userMessage"], json!({"@path": "/label"}));
        assert_eq!(wire["wantResponse"], true);
        assert_eq!(wire["responsePath"], "/reply");
        assert!(wire.get("response_path").is_none());
    }

    #[test]
    fn accessibility_retains_live_and_hidden() {
        let value = json!({"live": "polite", "hidden": {"@path": "/hide"}});
        let attributes: AccessibilityAttributes = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(attributes.live, Some(LiveRegion::Polite));
        assert_eq!(serde_json::to_value(attributes).unwrap(), value);
        assert!(
            serde_json::from_value::<AccessibilityAttributes>(json!({"live": "loud"})).is_err()
        );
    }
}
