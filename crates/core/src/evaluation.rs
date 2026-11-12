//! Opt-in resumable current-protocol expression evaluation.
//!
//! Unlike the legacy synchronous display API, pending remote values are explicit.
//! Hosts retain reactive handles and consume updates; each action handle is a
//! distinct invocation. This prototype does not mutate component properties.
use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::error::{A2uiError, Result};
use crate::message_processor::{MessageProcessor, RendererCall};
use crate::protocol::common_types::{DataBinding, FunctionCall};
use crate::protocol::rpc::{CallAgentFunction, FunctionOutcome, FunctionResponse};

/// An opaque host-owned subscription/invocation identifier.
pub type EvaluationId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluationMode {
    /// Subscribe to data changes and share identical in-flight agent calls.
    Reactive,
    /// A fresh imperative invocation, granted trusted user activation by the host.
    Action,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EvaluationStatus {
    Ready(Value),
    Pending,
    Cancelled,
    Error { code: String, message: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationUpdate {
    pub id: EvaluationId,
    pub status: EvaluationStatus,
}

/// Non-fatal expression diagnostics, such as a failed part of a formatted string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationDiagnostic {
    pub id: EvaluationId,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CallKey {
    surface: String,
    catalog: String,
    name: String,
    args: String,
    // Action expressions are neither shared across invocations nor across nodes.
    imperative: Option<(EvaluationId, String)>,
}

struct CallState {
    id: String,
    call: FunctionCall,
    status: EvaluationStatus,
}

struct Job {
    id: EvaluationId,
    surface: String,
    base_path: String,
    expression: Value,
    mode: EvaluationMode,
    status: Option<EvaluationStatus>,
    calls: Vec<CallKey>,
    touched: Vec<CallKey>,
    failures: HashMap<String, (CallKey, EvaluationStatus)>,
    suspended: bool,
    diagnostics: Vec<EvaluationDiagnostic>,
    // Avoid repeating a completed native action while waiting on another arg.
    memo: HashMap<String, Value>,
}

/// Processor-owned state. All calls stay bounded to active evaluation handles.
#[derive(Default)]
pub(crate) struct Evaluator {
    jobs: HashMap<EvaluationId, Job>,
    calls: HashMap<CallKey, CallState>,
    updates: Vec<EvaluationUpdate>,
    outgoing: Vec<CallAgentFunction>,
    cancelled_calls: Vec<String>,
    diagnostics: Vec<EvaluationDiagnostic>,
}

impl Evaluator {
    /// Reserve evaluator-owned IDs against explicit host RPC aliases.
    pub(crate) fn contains_call_id(&self, id: &str) -> bool {
        self.calls.values().any(|call| call.id == id)
    }

    fn push_update(&mut self, update: EvaluationUpdate) {
        if let Some(existing) = self
            .updates
            .iter_mut()
            .find(|existing| existing.id == update.id)
        {
            *existing = update;
        } else {
            self.updates.push(update);
        }
    }

    pub(crate) fn start(
        &mut self,
        surface: &str,
        base_path: &str,
        expression: Value,
        mode: EvaluationMode,
    ) -> Result<EvaluationId> {
        if self.jobs.len() >= 1024 || self.updates.len() >= 4096 || self.diagnostics.len() >= 4096 {
            return Err(A2uiError::Validation(
                "evaluation capacity reached; cancel handles and drain updates".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.jobs.insert(
            id.clone(),
            Job {
                id: id.clone(),
                surface: surface.into(),
                base_path: base_path.into(),
                expression,
                mode,
                status: None,
                calls: Vec::new(),
                touched: Vec::new(),
                failures: HashMap::new(),
                suspended: false,
                diagnostics: Vec::new(),
                memo: HashMap::new(),
            },
        );
        Ok(id)
    }

    pub(crate) fn latest_status(&self, id: &str) -> Option<EvaluationStatus> {
        self.jobs
            .get(id)
            .and_then(|job| job.status.clone())
            .or_else(|| {
                self.updates
                    .iter()
                    .find(|update| update.id == id)
                    .map(|update| update.status.clone())
            })
    }

    pub(crate) fn forget(&mut self, id: &str) {
        self.jobs.remove(id);
        self.updates.retain(|update| update.id != id);
        self.collect_calls();
    }

    pub(crate) fn cancel(&mut self, id: &str) {
        if self.jobs.remove(id).is_some() {
            self.push_update(EvaluationUpdate {
                id: id.into(),
                status: EvaluationStatus::Cancelled,
            });
        }
        self.collect_calls();
    }

    pub(crate) fn invalidate_surface(&mut self, surface: &str, deleted: bool) {
        let ids: Vec<_> = self
            .jobs
            .values()
            .filter(|job| job.surface == surface)
            .map(|job| job.id.clone())
            .collect();
        for id in ids {
            let cancel = deleted
                || self
                    .jobs
                    .get(&id)
                    .is_some_and(|job| job.mode == EvaluationMode::Action);
            if cancel {
                self.cancel(&id);
            } else if let Some(job) = self.jobs.get_mut(&id) {
                job.suspended = false;
                job.memo.clear();
            }
        }
    }

    pub(crate) fn cancel_surface_calls(&mut self, surface: &str) {
        self.invalidate_surface(surface, false);
        for job in self.jobs.values_mut().filter(|job| job.surface == surface) {
            job.suspended = true;
            job.calls.clear();
            job.failures.clear();
        }
        self.collect_calls();
    }

    pub(crate) fn response(
        &mut self,
        processor: &MessageProcessor,
        response: &FunctionResponse,
    ) -> Option<(String, String, String)> {
        // Unknown, cancelled, duplicate and stale generation responses are inert.
        if let Some((key, call)) = self
            .calls
            .iter_mut()
            .find(|(_, call)| call.id == response.function_call_id)
            && call.status == EvaluationStatus::Pending
        {
            call.status = match &response.outcome {
                FunctionOutcome::Success(value) => match processor.validate_renderer_return(
                    &key.surface,
                    &call.call,
                    value.value.clone(),
                ) {
                    Ok(value) => EvaluationStatus::Ready(value),
                    Err(e) => evaluation_error(e),
                },
                FunctionOutcome::Failure(failure) => {
                    error("EXPRESSION_ERROR", &failure.error.message)
                }
            };
            if let EvaluationStatus::Error { code, message } = &call.status {
                return Some((key.surface.clone(), code.clone(), message.clone()));
            }
        }
        None
    }

    pub(crate) fn advance(&mut self, processor: &MessageProcessor) -> Vec<CallAgentFunction> {
        let ids: Vec<_> = self.jobs.keys().cloned().collect();
        for id in ids {
            let Some(mut job) = self.jobs.remove(&id) else {
                continue;
            };
            if job.suspended {
                self.jobs.insert(id, job);
                continue;
            }
            job.touched.clear();
            let previous_diagnostics = std::mem::take(&mut job.diagnostics);
            let expression = job.expression.clone();
            let status = self.evaluate(processor, &mut job, &expression, "", 0);
            if job.status.as_ref() != Some(&status) {
                self.push_update(EvaluationUpdate {
                    id: id.clone(),
                    status: status.clone(),
                });
            }
            for diagnostic in &job.diagnostics {
                if !previous_diagnostics.contains(diagnostic)
                    && !self.diagnostics.contains(diagnostic)
                {
                    self.diagnostics.push(diagnostic.clone());
                }
            }
            job.status = Some(status.clone());
            job.calls = std::mem::take(&mut job.touched);
            if job.mode == EvaluationMode::Reactive || status == EvaluationStatus::Pending {
                self.jobs.insert(id, job);
            }
        }
        self.collect_calls();
        std::mem::take(&mut self.outgoing)
    }

    fn collect_calls(&mut self) {
        self.calls.retain(|key, call| {
            if self.jobs.values().any(|job| job.calls.contains(key)) {
                true
            } else {
                self.cancelled_calls.push(call.id.clone());
                false
            }
        });
        self.outgoing.retain(|call| {
            self.calls
                .values()
                .any(|state| state.id == call.function_call_id)
        });
    }

    pub(crate) fn drain_diagnostics(&mut self) -> Vec<EvaluationDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    pub(crate) fn drain_cancelled_calls(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cancelled_calls)
    }

    pub(crate) fn drain(&mut self) -> Vec<EvaluationUpdate> {
        std::mem::take(&mut self.updates)
    }

    fn evaluate(
        &mut self,
        processor: &MessageProcessor,
        job: &mut Job,
        value: &Value,
        node: &str,
        depth: usize,
    ) -> EvaluationStatus {
        if depth > 128 {
            return error("EXPRESSION_ERROR", "Expression nesting exceeds 128 levels");
        }
        if let Some(value) = job.memo.get(node) {
            return EvaluationStatus::Ready(value.clone());
        }
        let result = self.evaluate_inner(processor, job, value, node, depth);
        if job.mode == EvaluationMode::Action
            && let EvaluationStatus::Ready(value) = &result
        {
            job.memo.insert(node.into(), value.clone());
        }
        result
    }

    fn evaluate_inner(
        &mut self,
        processor: &MessageProcessor,
        job: &mut Job,
        value: &Value,
        node: &str,
        depth: usize,
    ) -> EvaluationStatus {
        match value {
            Value::Array(items) => {
                let statuses = items
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        self.evaluate(processor, job, value, &format!("{node}/{index}"), depth + 1)
                    })
                    .collect::<Vec<_>>();
                collect(statuses).map_ready(Value::Array)
            }
            Value::Object(object) if object.contains_key("@path") => {
                let binding: DataBinding = match serde_json::from_value(value.clone()) {
                    Ok(binding) => binding,
                    Err(e) => return error("EXPRESSION_ERROR", &e.to_string()),
                };
                let Some(surface) = processor.model.get_surface(&job.surface) else {
                    return EvaluationStatus::Cancelled;
                };
                let path = if binding.path.starts_with('/') {
                    binding.path
                } else if binding.path.is_empty() {
                    job.base_path.clone()
                } else {
                    format!("{}/{}", job.base_path.trim_end_matches('/'), binding.path)
                };
                EvaluationStatus::Ready(
                    surface
                        .data_model
                        .borrow()
                        .get(&path)
                        .cloned()
                        .unwrap_or(Value::Null),
                )
            }
            Value::Object(object) if object.contains_key("@call") => {
                let mut call: FunctionCall = match serde_json::from_value(value.clone()) {
                    Ok(call) => call,
                    Err(e) => return error("EXPRESSION_ERROR", &e.to_string()),
                };
                let mut args = HashMap::new();
                let mut pending = false;
                let mut failure = None;
                // Visit all siblings before returning Pending; independent RPCs run in parallel.
                for (name, value) in &call.args {
                    match self.evaluate(
                        processor,
                        job,
                        value,
                        &format!("{node}/args/{}", escape(name)),
                        depth + 1,
                    ) {
                        EvaluationStatus::Ready(value) => {
                            args.insert(name.clone(), value);
                        }
                        EvaluationStatus::Pending => pending = true,
                        status @ (EvaluationStatus::Error { .. } | EvaluationStatus::Cancelled) => {
                            failure = Some(status)
                        }
                    }
                }
                if let Some(failure) = failure {
                    return failure;
                }
                if pending {
                    return EvaluationStatus::Pending;
                }
                call.args = args;
                let surface_catalog = match processor.model.get_surface(&job.surface) {
                    Some(surface) => surface.catalog_id.clone(),
                    None => return EvaluationStatus::Cancelled,
                };
                let catalog = call.catalog_id.clone().unwrap_or(surface_catalog);
                let args: Map<_, _> = call
                    .args
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let key = CallKey {
                    surface: job.surface.clone(),
                    catalog,
                    name: call.call.clone(),
                    args: canonical(Value::Object(args)).to_string(),
                    imperative: (job.mode == EvaluationMode::Action)
                        .then(|| (job.id.clone(), node.into())),
                };
                if let Some((failed_key, failure)) = job.failures.get(node)
                    && failed_key == &key
                {
                    return failure.clone();
                }
                job.failures.remove(node);
                if let Some(state) = self.calls.get(&key) {
                    let status = state.status.clone();
                    if matches!(status, EvaluationStatus::Error { .. }) {
                        if job.calls.contains(&key) {
                            job.failures.insert(node.into(), (key, status.clone()));
                            return status;
                        }
                        // A new binding retries a failed call instead of inheriting
                        // another subscriber's settled expression error.
                    } else {
                        if !job.touched.contains(&key) {
                            job.touched.push(key);
                        }
                        return status;
                    }
                }
                let errors = std::cell::RefCell::new(Vec::new());
                let prepared = processor.prepare_renderer_call_with_errors(
                    &job.surface,
                    &job.base_path,
                    call,
                    job.mode == EvaluationMode::Action,
                    &errors,
                );
                for failure in errors.into_inner() {
                    if let EvaluationStatus::Error { code, message } = evaluation_error(failure) {
                        job.diagnostics.push(EvaluationDiagnostic {
                            id: job.id.clone(),
                            code,
                            message,
                        });
                    }
                }
                match prepared {
                    Ok(RendererCall::Local(value)) => EvaluationStatus::Ready(value),
                    Ok(RendererCall::Remote(call_function)) => {
                        if !processor.agent_transport_available() {
                            return error(
                                "EXPRESSION_ERROR",
                                &format!(
                                    "No outbound listener is available for agent function {}",
                                    call_function.call
                                ),
                            );
                        }
                        if self.calls.len() >= 4096 {
                            return error(
                                "EXPRESSION_ERROR",
                                "Too many remote expression calls (limit 4096)",
                            );
                        }
                        let id = uuid::Uuid::new_v4().to_string();
                        self.outgoing.push(CallAgentFunction {
                            surface_id: job.surface.clone(),
                            function_call_id: id.clone(),
                            call_function: call_function.clone(),
                        });
                        self.calls.insert(
                            key.clone(),
                            CallState {
                                id,
                                call: call_function,
                                status: EvaluationStatus::Pending,
                            },
                        );
                        job.touched.push(key);
                        EvaluationStatus::Pending
                    }
                    Err(e) => evaluation_error(e),
                }
            }
            Value::Object(object) => {
                let mut keys = Vec::new();
                let mut statuses = Vec::new();
                for (key, value) in object {
                    if key.starts_with('@') && !key.starts_with("@@") {
                        return error(
                            "EXPRESSION_ERROR",
                            &format!("Unknown reserved dynamic key: {key}"),
                        );
                    }
                    keys.push(
                        key.strip_prefix("@@")
                            .map(|tail| format!("@{tail}"))
                            .unwrap_or_else(|| key.clone()),
                    );
                    statuses.push(self.evaluate(
                        processor,
                        job,
                        value,
                        &format!("{node}/{}", escape(key)),
                        depth + 1,
                    ));
                }
                collect(statuses)
                    .map_ready(|values| Value::Object(keys.into_iter().zip(values).collect()))
            }
            value => EvaluationStatus::Ready(value.clone()),
        }
    }
}

// A small aggregate keeps sibling scheduling separate from value assembly.
enum Collected {
    Values(Vec<Value>),
    Other(EvaluationStatus),
}
impl Collected {
    fn map_ready(self, map: impl FnOnce(Vec<Value>) -> Value) -> EvaluationStatus {
        match self {
            Self::Values(values) => EvaluationStatus::Ready(map(values)),
            Self::Other(status) => status,
        }
    }
}
fn collect(statuses: Vec<EvaluationStatus>) -> Collected {
    if let Some(status) = statuses
        .iter()
        .find(|s| matches!(s, EvaluationStatus::Error { .. }))
    {
        return Collected::Other(status.clone());
    }
    if statuses.contains(&EvaluationStatus::Cancelled) {
        return Collected::Other(EvaluationStatus::Cancelled);
    }
    if statuses.contains(&EvaluationStatus::Pending) {
        return Collected::Other(EvaluationStatus::Pending);
    }
    Collected::Values(
        statuses
            .into_iter()
            .filter_map(|s| {
                if let EvaluationStatus::Ready(v) = s {
                    Some(v)
                } else {
                    None
                }
            })
            .collect(),
    )
}
fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn error(code: &str, message: &str) -> EvaluationStatus {
    EvaluationStatus::Error {
        code: code.into(),
        message: message.into(),
    }
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonical(v)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        value => value,
    }
}

fn evaluation_error(error_value: A2uiError) -> EvaluationStatus {
    let code = if matches!(error_value, A2uiError::CatalogNotFound(_)) {
        "CATALOG_ERROR"
    } else {
        "EXPRESSION_ERROR"
    };
    error(code, &error_value.to_string())
}
