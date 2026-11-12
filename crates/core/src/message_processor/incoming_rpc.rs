//! Incoming RPC boundaries; activation can only be supplied by the trusted host.
use super::MessageProcessor;
use crate::{
    error::{A2uiError, Result},
    protocol::{
        client_to_server::{ClientMessage, ClientPayload, ErrorData, ErrorPayload},
        rpc::CallRendererFunction,
    },
};

pub(super) struct InvocationFailure {
    pub error: A2uiError,
    pub execution: bool,
}
impl From<A2uiError> for InvocationFailure {
    fn from(error: A2uiError) -> Self {
        Self {
            error,
            execution: false,
        }
    }
}
impl InvocationFailure {
    pub fn execution(error: A2uiError) -> Self {
        Self {
            error,
            execution: true,
        }
    }
}

impl MessageProcessor {
    /// Process wire JSON and report correlated malformed RPC envelopes to the peer.
    /// Unlike `parse_message`, this has processor state and can queue a wire error.
    pub fn process_json_message(&mut self, json: &str) -> Result<()> {
        match Self::parse_message(json) {
            Ok(message) => self.process_message(message),
            Err(error) => {
                if let Ok(raw) = serde_json::from_str::<serde_json::Value>(json) {
                    if raw["version"] == "1.0" {
                        if let Some(id) = raw
                            .pointer("/callRendererFunction/functionCallId")
                            .and_then(|v| v.as_str())
                        {
                            self.queue_outgoing(ClientMessage {
                                version: "1.0".into(),
                                payload: ClientPayload::Error(ErrorPayload {
                                    error: ErrorData {
                                        code: "INVALID_FUNCTION_CALL".into(),
                                        message: error.to_string(),
                                        surface_id: None,
                                        function_call_id: Some(id.into()),
                                    },
                                }),
                            });
                        }
                    }
                }
                Err(error)
            }
        }
    }

    /// Invoke a current incoming renderer RPC during an actual host user gesture.
    /// Never derive permission from an incoming message or agent-supplied flag.
    /// Ordinary wire processing always runs without activation.
    pub fn process_renderer_call_from_user_gesture(
        &mut self,
        call: &CallRendererFunction,
    ) -> Result<()> {
        self.handle_renderer_function_with_activation(call, true)
    }
}
