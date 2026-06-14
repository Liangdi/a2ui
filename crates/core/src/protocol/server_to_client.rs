//! A2UI v1.0 Server-to-Client Messages
//!
//! Mirrors the JSON Schema `server_to_client.json`.

use serde::{Deserialize, Serialize};

use super::common_types::FunctionCall;

// ---------------------------------------------------------------------------
// Top-level envelope
// ---------------------------------------------------------------------------

/// A single A2UI message from the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct A2uiMessage {
    pub version: String,
    #[serde(flatten)]
    pub payload: A2uiPayload,
}

/// The typed payload of a server-to-client message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum A2uiPayload {
    CreateSurface(CreateSurfacePayload),
    UpdateComponents(UpdateComponentsPayload),
    UpdateDataModel(UpdateDataModelPayload),
    DeleteSurface(DeleteSurfacePayload),
    CallFunction(CallFunctionPayload),
    ActionResponse(ActionResponsePayload),
}

// ---------------------------------------------------------------------------
// createSurface
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateSurfacePayload {
    pub create_surface: CreateSurfaceData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateSurfaceData {
    pub surface_id: String,
    pub catalog_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_properties: Option<serde_json::Value>,
    #[serde(default)]
    pub send_data_model: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_model: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// updateComponents
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateComponentsPayload {
    pub update_components: UpdateComponentsData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateComponentsData {
    pub surface_id: String,
    pub components: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// updateDataModel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDataModelPayload {
    pub update_data_model: UpdateDataModelData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDataModelData {
    pub surface_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// deleteSurface
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSurfacePayload {
    pub delete_surface: DeleteSurfaceData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSurfaceData {
    pub surface_id: String,
}

// ---------------------------------------------------------------------------
// callFunction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallFunctionPayload {
    pub call_function: FunctionCall,
    pub function_call_id: String,
    #[serde(default)]
    pub want_response: bool,
}

// ---------------------------------------------------------------------------
// actionResponse
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionResponsePayload {
    pub action_response: ActionResponseData,
    pub action_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionResponseData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ActionResponseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionResponseError {
    pub code: String,
    pub message: String,
}
