//! Role-based RPC envelopes from the released A2UI 1.0 specification.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::common_types::FunctionCall;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallRendererFunction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
    pub function_call_id: String,
    pub call_function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallAgentFunction {
    pub surface_id: String,
    pub function_call_id: String,
    pub call_function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    pub function_call_id: String,
    #[serde(flatten)]
    pub outcome: FunctionOutcome,
}

/// Exactly one of `value` (including JSON null) or `error`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum FunctionOutcome {
    Success(FunctionValue),
    Failure(FunctionFailure),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FunctionValue {
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FunctionFailure {
    pub error: FunctionError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FunctionError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallRendererFunctionPayload {
    pub call_renderer_function: CallRendererFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentFunctionResponsePayload {
    pub agent_function_response: FunctionResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallAgentFunctionPayload {
    pub call_agent_function: CallAgentFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RendererFunctionResponsePayload {
    pub renderer_function_response: FunctionResponse,
}

impl<'de> Deserialize<'de> for FunctionOutcome {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let object = raw
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("function result must be an object"))?;
        if object.len() != 1 {
            return Err(serde::de::Error::custom(
                "function result requires exactly one of value or error",
            ));
        }
        if let Some(value) = object.get("value") {
            Ok(Self::Success(FunctionValue {
                value: value.clone(),
            }))
        } else if let Some(error) = object.get("error") {
            Ok(Self::Failure(FunctionFailure {
                error: serde_json::from_value(error.clone()).map_err(serde::de::Error::custom)?,
            }))
        } else {
            Err(serde::de::Error::custom(
                "function result requires value or error",
            ))
        }
    }
}
