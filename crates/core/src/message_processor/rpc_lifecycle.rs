//! Explicit host RPC lifetime management. Reactive evaluator calls retain their
//! independent subscription lifecycle; this state owns only explicit RPC IDs.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use super::MessageProcessor;
use crate::protocol::rpc::{
    CallAgentFunction, FunctionError, FunctionFailure, FunctionOutcome, FunctionResponse,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct RpcCallError {
    pub code: &'static str,
    pub message: String,
}
impl RpcCallError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

struct PendingCall {
    surface: String,
    deadline: Option<Instant>,
}
#[derive(Default)]
pub(super) struct RpcPendingState {
    pending: HashMap<String, PendingCall>,
    results: Vec<FunctionResponse>,
    settled_ids: Vec<String>,
    disposed: bool,
}
impl RpcPendingState {
    fn begin(
        &mut self,
        call: &CallAgentFunction,
        timeout: Option<Duration>,
        now: Instant,
    ) -> Result<(), RpcCallError> {
        if self.disposed {
            return Err(RpcCallError::new("DISPOSED", "RPC state has been disposed"));
        }
        self.expire(now);
        if self.pending.contains_key(&call.function_call_id) {
            return Err(RpcCallError::new(
                "DUPLICATE",
                "functionCallId is already pending",
            ));
        }
        let deadline = timeout
            .map(|duration| {
                now.checked_add(duration).ok_or_else(|| {
                    RpcCallError::new(
                        "INVALID_FUNCTION_CALL",
                        "RPC timeout exceeds the monotonic clock range",
                    )
                })
            })
            .transpose()?;
        self.pending.insert(
            call.function_call_id.clone(),
            PendingCall {
                surface: call.surface_id.clone(),
                deadline,
            },
        );
        Ok(())
    }
    fn finish(&mut self, response: FunctionResponse) -> bool {
        if self.pending.remove(&response.function_call_id).is_none() {
            return false;
        }
        self.settled_ids.push(response.function_call_id.clone());
        self.results.push(response);
        true
    }
    pub(super) fn response(&mut self, response: &FunctionResponse, now: Instant) -> bool {
        self.expire(now);
        self.finish(response.clone())
    }
    fn reject(&mut self, id: String, code: &str, message: &str) {
        self.finish(FunctionResponse {
            function_call_id: id,
            outcome: FunctionOutcome::Failure(FunctionFailure {
                error: FunctionError {
                    code: code.into(),
                    message: message.into(),
                },
            }),
        });
    }
    fn expire(&mut self, now: Instant) {
        let ids: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, p)| p.deadline.is_some_and(|deadline| deadline <= now))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.reject(id, "TIMEOUT", "Agent function call timed out");
        }
    }
    pub(super) fn cancel_surface(&mut self, surface: &str) {
        let ids: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, p)| p.surface == surface)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.reject(id, "CANCELLED", "Surface function calls were cancelled");
        }
    }
    fn dispose(&mut self) {
        self.disposed = true;
        let ids: Vec<_> = self.pending.keys().cloned().collect();
        for id in ids {
            self.reject(id, "CANCELLED", "RPC state was disposed");
        }
    }
}

impl MessageProcessor {
    /// Begin an explicit renderer-to-agent call. Deadlines use a monotonic clock;
    /// the host must call `expire_agent_calls` from its timer/event loop. Evaluator
    /// calls are not tracked here and remain controlled by their subscriptions.
    pub fn begin_agent_call(
        &mut self,
        call: CallAgentFunction,
        timeout: Option<Duration>,
        now: Instant,
    ) -> Result<(), RpcCallError> {
        if self.rpc_pending.disposed {
            return Err(RpcCallError::new("DISPOSED", "RPC state has been disposed"));
        }
        if !self.agent_transport_available {
            return Err(RpcCallError::new(
                "NO_LISTENER",
                "No outbound listener is available",
            ));
        }
        if self.model.get_surface(&call.surface_id).is_none() {
            return Err(RpcCallError::new(
                "SURFACE_NOT_FOUND",
                format!("Surface not found: {}", call.surface_id),
            ));
        }
        if self.evaluator.contains_call_id(&call.function_call_id) {
            return Err(RpcCallError::new(
                "DUPLICATE",
                "functionCallId belongs to an evaluator call",
            ));
        }
        let result = self.rpc_pending.begin(&call, timeout, now);
        self.prune_settled_agent_calls();
        result?;
        self.queue_outgoing(crate::protocol::client_to_server::ClientMessage {
            version: "1.0".into(),
            payload: crate::protocol::client_to_server::ClientPayload::CallAgentFunction(
                crate::protocol::rpc::CallAgentFunctionPayload {
                    call_agent_function: call,
                },
            ),
        });
        Ok(())
    }
    /// Expire elapsed explicit RPC deadlines. No wall-clock sleeps are required.
    pub fn expire_agent_calls(&mut self, now: Instant) {
        self.rpc_pending.expire(now);
        self.prune_settled_agent_calls();
    }
    /// Cancel explicit calls and permanently close this explicit RPC manager.
    /// A subsequent processor `reset` creates a fresh manager. Reactive evaluator
    /// subscriptions are separate and are not disposed by this method.
    pub fn dispose_agent_calls(&mut self) {
        self.rpc_pending.dispose();
        self.prune_settled_agent_calls();
    }
    /// Drain terminal values/errors once, including TIMEOUT and CANCELLED outcomes.
    pub fn drain_agent_call_results(&mut self) -> Vec<FunctionResponse> {
        std::mem::take(&mut self.rpc_pending.results)
    }
    pub(super) fn prune_settled_agent_calls(&mut self) {
        let ids = std::mem::take(&mut self.rpc_pending.settled_ids);
        self.outgoing_messages
            .retain(|message| match &message.payload {
                crate::protocol::client_to_server::ClientPayload::CallAgentFunction(payload) => {
                    !ids.contains(&payload.call_agent_function.function_call_id)
                }
                _ => true,
            });
    }
}
