//! Current-protocol action/check orchestration over resumable expressions.
use std::collections::{HashMap, VecDeque};

use serde_json::Value;

use super::MessageProcessor;
use crate::error::{A2uiError, Result};
use crate::evaluation::{EvaluationMode, EvaluationStatus};
use crate::protocol::client_to_server::{
    ActionData, ActionPayload, ClientMessage, ClientPayload, ErrorData, ErrorPayload,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionError {
    pub surface_id: String,
    pub component_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckUpdate {
    pub id: String,
    /// Pending checks do not fail validation.
    pub pending: bool,
    pub valid: bool,
    pub error: Option<ActionError>,
}

struct Check {
    surface: String,
    last: Option<CheckUpdate>,
}

struct PendingAction {
    completion_id: Option<String>,
    return_value: Value,
    surface: String,
    component: String,
    base_path: String,
    timestamp: String,
    name: Option<String>,
    entries: VecDeque<(Entry, Value)>,
    current: Option<(Entry, String)>,
    context: HashMap<String, Value>,
    user_message: Option<String>,
}

#[derive(Clone)]
enum Entry {
    Context(String),
    UserMessage,
    Function,
}

#[derive(Default)]
pub(super) struct AsyncActions {
    actions: Vec<PendingAction>,
    checks: HashMap<String, Check>,
    check_updates: Vec<CheckUpdate>,
    errors: Vec<ActionError>,
    completed: HashMap<String, std::result::Result<Value, ActionError>>,
}

impl MessageProcessor {
    /// Start a current action from a trusted user gesture. Event context entries
    /// resolve sequentially in wire order, followed by userMessage. No generated
    /// input can grant itself activation by calling this API through a message.
    pub fn start_current_action(
        &mut self,
        surface: &str,
        component: &str,
        base_path: &str,
        action: Value,
    ) -> Result<()> {
        self.start_current_action_inner(surface, component, base_path, action, None)
    }

    /// Start a trusted host action and retain its terminal result under an opaque
    /// handle. At most 1024 pending/completed tracked actions are retained. Drain
    /// completed handles with `take_current_action_result` to release capacity.
    /// Event actions complete with null after dispatch; function actions retain
    /// the actual function result. This does not grant activation to wire input.
    pub fn start_current_action_with_result(
        &mut self,
        surface: &str,
        component: &str,
        base_path: &str,
        action: Value,
    ) -> Result<String> {
        if self.model.get_surface(surface).is_none() {
            return Err(A2uiError::SurfaceNotFound(surface.into()));
        }
        let tracked = self
            .async_actions
            .actions
            .iter()
            .filter(|action| action.completion_id.is_some())
            .count()
            + self.async_actions.completed.len();
        if tracked >= 1024 {
            return Err(A2uiError::Validation(
                "action result capacity reached; take completed results".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.start_current_action_inner(surface, component, base_path, action, Some(id.clone()))?;
        Ok(id)
    }

    /// Take a terminal result once. None means pending, unknown, or already taken.
    /// Cancelled actions return a local CANCELLED result without a wire error.
    pub fn take_current_action_result(
        &mut self,
        id: &str,
    ) -> Option<std::result::Result<Value, ActionError>> {
        self.async_actions.completed.remove(id)
    }

    fn complete_current_action(
        &mut self,
        action: &PendingAction,
        result: std::result::Result<Value, ActionError>,
    ) {
        if let Some(id) = &action.completion_id {
            self.async_actions.completed.insert(id.clone(), result);
        }
    }

    fn cancel_current_action_result(&mut self, action: &PendingAction) {
        self.complete_current_action(
            action,
            Err(ActionError {
                surface_id: action.surface.clone(),
                component_id: action.component.clone(),
                code: "CANCELLED".into(),
                message: "Action was cancelled".into(),
            }),
        );
    }

    fn start_current_action_inner(
        &mut self,
        surface: &str,
        component: &str,
        base_path: &str,
        action: Value,
        completion_id: Option<String>,
    ) -> Result<()> {
        if self.model.get_surface(surface).is_none() {
            return Ok(());
        }
        if self.async_actions.actions.len() >= 1024 || self.async_actions.errors.len() >= 4096 {
            return Err(A2uiError::Validation(
                "action capacity reached; complete pending actions and drain errors".into(),
            ));
        }
        let object = action
            .as_object()
            .ok_or_else(|| A2uiError::Validation("action must be an object".into()))?;
        let mut entries = VecDeque::new();
        let name = if let Some(event) = object.get("event") {
            if object.contains_key("functionCall") {
                return Err(A2uiError::Validation(
                    "action must have exactly one kind".into(),
                ));
            }
            let name = event
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| A2uiError::Validation("event action requires a name".into()))?;
            if let Some(context) = event.get("context") {
                let context = context.as_object().ok_or_else(|| {
                    A2uiError::Validation("event context must be an object".into())
                })?;
                for (key, value) in context {
                    entries.push_back((Entry::Context(key.clone()), value.clone()));
                }
            }
            if let Some(value) = event.get("userMessage") {
                entries.push_back((Entry::UserMessage, value.clone()));
            }
            Some(name.to_owned())
        } else if let Some(function) = object.get("functionCall") {
            entries.push_back((Entry::Function, function.clone()));
            None
        } else {
            return Err(A2uiError::Validation("unknown action kind".into()));
        };
        if entries.len() > 1024 {
            return Err(A2uiError::Validation(
                "too many action context entries".into(),
            ));
        }
        self.async_actions.actions.push(PendingAction {
            completion_id,
            return_value: Value::Null,
            surface: surface.into(),
            component: component.into(),
            base_path: base_path.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            name,
            entries,
            current: None,
            context: HashMap::new(),
            user_message: None,
        });
        self.advance_evaluations();
        Ok(())
    }

    /// Subscribe to a check condition. Pending is non-failing; errors fail closed.
    /// The condition is evaluated passively and cannot acquire user activation.
    pub fn start_check(
        &mut self,
        surface: &str,
        base_path: &str,
        condition: Value,
    ) -> Result<String> {
        if self.model.get_surface(surface).is_none() {
            return Err(A2uiError::SurfaceNotFound(surface.into()));
        }
        let id = self
            .evaluator
            .start(surface, base_path, condition, EvaluationMode::Reactive)?;
        self.async_actions.checks.insert(
            id.clone(),
            Check {
                surface: surface.into(),
                last: None,
            },
        );
        self.advance_evaluations();
        Ok(id)
    }

    pub fn cancel_check(&mut self, id: &str) {
        self.async_actions.checks.remove(id);
        self.async_actions
            .check_updates
            .retain(|update| update.id != id);
        self.evaluator.forget(id);
        self.remove_cancelled_evaluation_calls();
    }

    /// Read a check subscription without consuming the host update queue.
    pub fn check_status(&self, id: &str) -> Option<CheckUpdate> {
        self.async_actions
            .checks
            .get(id)
            .and_then(|check| check.last.clone())
    }

    pub fn drain_check_updates(&mut self) -> Vec<CheckUpdate> {
        std::mem::take(&mut self.async_actions.check_updates)
    }
    pub fn drain_action_errors(&mut self) -> Vec<ActionError> {
        std::mem::take(&mut self.async_actions.errors)
    }

    fn report_action_error(
        &mut self,
        action: &PendingAction,
        code: String,
        message: String,
        function_call_id: Option<String>,
    ) {
        let error = ActionError {
            surface_id: action.surface.clone(),
            component_id: action.component.clone(),
            code: code.clone(),
            message: message.clone(),
        };
        self.complete_current_action(action, Err(error.clone()));
        self.async_actions.errors.push(error);
        // The processor already emits a correlated wire error for failed remote
        // calls. Keep the action-specific diagnostic without emitting it twice.
        if let Some(id) = function_call_id.as_deref() {
            if let Some(existing) =
                self.outgoing_messages
                    .iter_mut()
                    .find_map(|outgoing| match &mut outgoing.payload {
                        ClientPayload::Error(payload)
                            if payload.error.function_call_id.as_deref() == Some(id) =>
                        {
                            Some(&mut payload.error)
                        }
                        _ => None,
                    })
            {
                existing.message = message;
                return;
            }
        }
        self.queue_outgoing(ClientMessage {
            version: "1.0".into(),
            payload: ClientPayload::Error(ErrorPayload {
                error: ErrorData {
                    code,
                    message,
                    surface_id: function_call_id.is_none().then(|| action.surface.clone()),
                    function_call_id,
                },
            }),
        });
    }

    /// Called after each evaluator pass. Returns true when a new expression was
    /// started, so the processor should run another pass before returning.
    pub(super) fn drive_async_actions(&mut self) -> bool {
        let mut started = false;
        let mut pending = Vec::new();
        for mut action in std::mem::take(&mut self.async_actions.actions) {
            if self.model.get_surface(&action.surface).is_none() {
                self.cancel_current_action_result(&action);
                if let Some((_, id)) = action.current {
                    self.evaluator.forget(&id);
                }
                continue;
            }
            let mut failed = false;
            if let Some((entry, id)) = action.current.take() {
                let status = self.evaluator.latest_status(&id);
                let function_call_id = self.evaluator.error_call_id(&id).map(str::to_owned);
                if matches!(
                    status,
                    Some(
                        EvaluationStatus::Ready(_)
                            | EvaluationStatus::Error { .. }
                            | EvaluationStatus::Cancelled
                    )
                ) {
                    self.evaluator.forget(&id);
                }
                match status {
                    Some(EvaluationStatus::Ready(value)) => match entry {
                        Entry::Context(key) => {
                            action.context.insert(key, value);
                        }
                        Entry::UserMessage => match value {
                            Value::Null => {}
                            Value::String(value) => action.user_message = Some(value),
                            _ => {
                                self.report_action_error(
                                    &action,
                                    "INVALID_ACTION".into(),
                                    "Action userMessage must resolve to a string or null".into(),
                                    None,
                                );
                                failed = true;
                            }
                        },
                        Entry::Function => action.return_value = value,
                    },
                    Some(EvaluationStatus::Error { code, message }) => {
                        let message = match entry {
                            Entry::Context(key) => {
                                format!("Action event context entry '{key}' is invalid: {message}")
                            }
                            _ => message,
                        };
                        self.report_action_error(&action, code, message, function_call_id);
                        failed = true;
                    }
                    Some(EvaluationStatus::Cancelled) => {
                        self.cancel_current_action_result(&action);
                        failed = true;
                    }
                    _ => {
                        action.current = Some((entry, id));
                        pending.push(action);
                        continue;
                    }
                }
            }
            if failed {
                continue;
            }
            if let Some((entry, expression)) = action.entries.pop_front() {
                match self.evaluator.start(
                    &action.surface,
                    &action.base_path,
                    expression,
                    EvaluationMode::Action,
                ) {
                    Ok(id) => {
                        action.current = Some((entry, id));
                        pending.push(action);
                        started = true;
                    }
                    Err(error) => self.report_action_error(
                        &action,
                        "EXPRESSION_ERROR".into(),
                        error.to_string(),
                        None,
                    ),
                }
            } else {
                self.complete_current_action(&action, Ok(action.return_value.clone()));
                if let Some(name) = action.name {
                    self.queue_outgoing(ClientMessage {
                        version: "1.0".into(),
                        payload: ClientPayload::Action(ActionPayload {
                            action: ActionData {
                                name,
                                surface_id: action.surface,
                                source_component_id: action.component,
                                timestamp: action.timestamp,
                                context: action.context,
                                user_message: action.user_message,
                                metadata: None,
                                want_response: None,
                                action_id: None,
                            },
                        }),
                    });
                }
            }
        }
        self.async_actions.actions = pending;
        let mut checks = std::mem::take(&mut self.async_actions.checks);
        checks.retain(|id, check| {
            if self.model.get_surface(&check.surface).is_none() {
                self.evaluator.forget(id);
                self.async_actions
                    .check_updates
                    .retain(|update| update.id != *id);
                return false;
            }
            let status = self.evaluator.latest_status(id);
            if status == Some(EvaluationStatus::Cancelled) {
                self.evaluator.forget(id);
                self.async_actions
                    .check_updates
                    .retain(|update| update.id != *id);
                return false;
            }
            let (pending, valid, error) = match status {
                Some(EvaluationStatus::Ready(value)) => (
                    false,
                    crate::model::data_context::value_to_boolean(&value),
                    None,
                ),
                Some(EvaluationStatus::Error { code, message }) => (
                    false,
                    false,
                    Some(ActionError {
                        surface_id: check.surface.clone(),
                        component_id: String::new(),
                        code,
                        message,
                    }),
                ),
                _ => (true, true, None),
            };
            let update = CheckUpdate {
                id: id.clone(),
                pending,
                valid,
                error,
            };
            if check.last.as_ref() != Some(&update) {
                if let Some(existing) = self
                    .async_actions
                    .check_updates
                    .iter_mut()
                    .find(|u| u.id == *id)
                {
                    *existing = update.clone();
                } else {
                    self.async_actions.check_updates.push(update.clone());
                }
                check.last = Some(update);
            }
            true
        });
        self.async_actions.checks = checks;
        self.remove_cancelled_evaluation_calls();
        started
    }
}
