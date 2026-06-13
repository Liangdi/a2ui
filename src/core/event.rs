//! Framework-agnostic input event types and interaction results.
//!
//! These types define the contract between the UI framework layer (e.g. ratatui)
//! and the core A2UI runtime, enabling keyboard events to be routed to components
//! and interaction results (actions, data updates) to flow back to the application.

use std::collections::HashMap;

/// A user input event, framework-agnostic.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// A key was pressed.
    KeyPress { key: InputKey },
}

/// Logical key identifiers, independent of any specific UI framework.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKey {
    /// A printable character.
    Char(char),
    /// Enter / Return key.
    Enter,
    /// Tab key.
    Tab,
    /// Shift+Tab.
    BackTab,
    /// Arrow keys.
    Up,
    Down,
    Left,
    Right,
    /// Backspace key.
    Backspace,
    /// Delete key.
    Delete,
    /// Escape key.
    Escape,
    /// Space bar.
    Space,
}

/// Result produced by a component handling an input event.
#[derive(Debug, Clone)]
pub enum EventResult {
    /// Dispatch an action to the server.
    Action {
        /// The event name.
        event_name: String,
        /// Context values to send with the event.
        context: HashMap<String, serde_json::Value>,
        /// Whether the action expects a response.
        want_response: bool,
        /// Optional data model path to write the response value.
        response_path: Option<String>,
    },
    /// Write a value back to the data model at the given path.
    DataUpdate {
        /// JSON Pointer path in the data model.
        path: String,
        /// The new value.
        value: serde_json::Value,
    },
    /// Toggle a boolean value in the data model.
    Toggle {
        /// JSON Pointer path in the data model.
        path: String,
    },
    /// Component consumed the event; no further processing needed.
    Consumed,
}
