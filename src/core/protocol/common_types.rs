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
/// Serialized as `{ "path": "/some/pointer" }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataBinding {
    pub path: String,
}

/// A named function call with arguments.
/// Each argument value can itself be any JSON value (including nested Dynamic values).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub call: String,
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
/// - Binding: `{ "path": "/user/name" }`
/// - Function: `{ "call": "capitalize", "args": { ... } }`
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

/// A general-purpose dynamic value — can be any JSON primitive, binding, or function call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<serde_json::Value>),
    Binding(DataBinding),
    Function(FunctionCall),
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
    FunctionCall { function_call: FunctionCall },
}

/// A server-bound event with optional context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionEvent {
    pub name: String,
    #[serde(default)]
    pub context: HashMap<String, DynamicValue>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub want_response: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub message: String,
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
