//! A2UI v1.0 Client-to-Server Messages
//!
//! Mirrors the JSON Schema `client_to_server.json`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Top-level envelope
// ---------------------------------------------------------------------------

/// A message sent from the client to the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientMessage {
    pub version: String,
    #[serde(flatten)]
    pub payload: ClientPayload,
}

/// The typed payload of a client-to-server message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ClientPayload {
    Action(ActionPayload),
    FunctionResponse(FunctionResponsePayload),
    Error(ErrorPayload),
}

// ---------------------------------------------------------------------------
// action
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionPayload {
    pub action: ActionData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionData {
    pub name: String,
    pub surface_id: String,
    pub source_component_id: String,
    pub timestamp: String,
    pub context: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub want_response: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
}

// ---------------------------------------------------------------------------
// functionResponse
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionResponsePayload {
    pub function_response: FunctionResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionResponseData {
    pub function_call_id: String,
    pub call: String,
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorPayload {
    pub error: ErrorData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorData {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_call_id: Option<String>,
}
