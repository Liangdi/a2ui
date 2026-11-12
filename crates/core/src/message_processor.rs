//! Message processor — parses A2UI messages and mutates state models.

use std::collections::HashMap;

mod async_actions;
mod renderer_actions;
pub use async_actions::{ActionError, CheckUpdate};
mod renderer_call;
pub use renderer_call::RendererCall;

use crate::catalog::Catalog;
use crate::error::{A2uiError, Result};
use crate::model::data_model::DataModel;
use crate::model::surface_group_model::SurfaceGroupModel;
use crate::model::surface_model::SurfaceModel;
use crate::protocol::client_to_server::{
    ClientMessage, ClientPayload, ErrorData, ErrorPayload, FunctionResponseData,
    FunctionResponsePayload,
};
use crate::protocol::rpc::*;
use crate::protocol::server_to_client::{
    A2uiMessage, A2uiPayload, CallFunctionPayload, CreateSurfaceData, DeleteSurfaceData,
    UpdateComponentsData, UpdateDataModelData,
};

/// Parses A2UI JSON messages and applies them to the state models.
pub struct MessageProcessor {
    /// The state model (all active surfaces).
    pub model: SurfaceGroupModel,
    /// Registered catalogs keyed by catalog ID.
    #[allow(dead_code)]
    catalogs: HashMap<String, Catalog>,
    catalog_versions: HashMap<String, String>,
    /// Outgoing client-to-server messages produced during processing.
    outgoing_messages: Vec<ClientMessage>,
    agent_function_responses: Vec<FunctionResponse>,
    evaluator: crate::evaluation::Evaluator,
    async_actions: async_actions::AsyncActions,
    agent_transport_available: bool,
    /// Validation config; `None` = validation fully OFF (the default, which
    /// preserves the existing graceful-degradation behavior — bad refs / dup
    /// ids / cycles are silently tolerated and components still load).
    validation: Option<crate::validate::ValidationConfig>,
    /// Accumulated validation diagnostics from the last batch of processed
    /// messages. Drained via [`Self::drain_validation`].
    pending_validation: crate::validate::ValidationReport,
}

impl MessageProcessor {
    /// Create a new processor with the given catalogs.
    pub fn new(catalogs: Vec<Catalog>) -> Self {
        let catalog_map: HashMap<String, Catalog> =
            catalogs.into_iter().map(|c| (c.id.clone(), c)).collect();
        Self {
            model: SurfaceGroupModel::new(),
            catalogs: catalog_map,
            catalog_versions: HashMap::new(),
            outgoing_messages: Vec::new(),
            agent_function_responses: Vec::new(),
            evaluator: crate::evaluation::Evaluator::default(),
            async_actions: async_actions::AsyncActions::default(),
            agent_transport_available: true,
            validation: None,
            pending_validation: crate::validate::ValidationReport::new(),
        }
    }

    /// Reset all processed state (surfaces and outgoing messages) while
    /// keeping the registered catalogs intact.
    ///
    /// Use this to replay a sample from scratch instead of constructing a new
    /// processor with `MessageProcessor::new(vec![])`, which would silently
    /// drop the catalogs and cause every component type to be flagged as
    /// "unknown".
    pub fn reset(&mut self) {
        self.model = SurfaceGroupModel::new();
        self.outgoing_messages.clear();
        self.agent_function_responses.clear();
        self.evaluator = crate::evaluation::Evaluator::default();
        self.async_actions = async_actions::AsyncActions::default();
        self.pending_validation = crate::validate::ValidationReport::new();
    }

    /// Declare whether the host can deliver newly queued agent RPCs. This does
    /// not create a connection. Defaults to true for existing queue-draining hosts.
    /// Already queued/transmitted calls are unaffected; cancel them explicitly.
    pub fn set_agent_transport_available(&mut self, available: bool) {
        self.agent_transport_available = available;
    }

    pub fn agent_transport_available(&self) -> bool {
        self.agent_transport_available
    }

    /// Start an opt-in current expression evaluation. Only pass Action for a
    /// trusted host user gesture; incoming messages must use Reactive.
    pub fn start_evaluation(
        &mut self,
        surface_id: &str,
        base_path: &str,
        expression: serde_json::Value,
        mode: crate::evaluation::EvaluationMode,
    ) -> Result<crate::evaluation::EvaluationId> {
        if self.model.get_surface(surface_id).is_none() {
            return Err(A2uiError::SurfaceNotFound(surface_id.into()));
        }
        let id = self
            .evaluator
            .start(surface_id, base_path, expression, mode)?;
        self.advance_evaluations();
        Ok(id)
    }

    /// Stop a reactive subscription or pending action; late responses are ignored.
    pub fn cancel_evaluation(&mut self, id: &str) {
        self.evaluator.cancel(id);
        self.remove_cancelled_evaluation_calls();
    }

    /// Cancel surface RPCs but retain reactive subscriptions. A later surface
    /// invalidation restarts their evaluation; stale replies remain inert.
    pub fn cancel_surface_calls(&mut self, surface_id: &str) {
        self.evaluator.cancel_surface_calls(surface_id);
        self.remove_cancelled_evaluation_calls();
    }

    /// Notify the evaluator after a host changes surface data directly rather
    /// than through `process_message`. Pending actions are cancelled safely.
    pub fn invalidate_evaluations(&mut self, surface_id: &str) {
        self.evaluator.invalidate_surface(surface_id, false);
        self.advance_evaluations();
    }

    /// Consume changed evaluation states (including explicit pending/errors).
    pub fn drain_evaluation_updates(&mut self) -> Vec<crate::evaluation::EvaluationUpdate> {
        self.evaluator.drain()
    }

    /// Drain non-fatal diagnostics independently of successful partial values.
    pub fn drain_evaluation_diagnostics(&mut self) -> Vec<crate::evaluation::EvaluationDiagnostic> {
        self.evaluator.drain_diagnostics()
    }

    fn remove_cancelled_evaluation_calls(&mut self) {
        let ids = self.evaluator.drain_cancelled_calls();
        self.outgoing_messages
            .retain(|message| match &message.payload {
                ClientPayload::CallAgentFunction(payload) => {
                    !ids.contains(&payload.call_agent_function.function_call_id)
                }
                _ => true,
            });
    }

    fn advance_evaluations(&mut self) {
        loop {
            let mut evaluator = std::mem::take(&mut self.evaluator);
            let calls = evaluator.advance(self);
            self.evaluator = evaluator;
            self.remove_cancelled_evaluation_calls();
            for call in calls {
                self.queue_outgoing(ClientMessage {
                    version: "1.0".into(),
                    payload: ClientPayload::CallAgentFunction(CallAgentFunctionPayload {
                        call_agent_function: call,
                    }),
                });
            }
            if !self.drive_async_actions() {
                break;
            }
        }
    }

    /// Parse a raw JSON string into an A2uiMessage.
    pub fn parse_message(json: &str) -> Result<A2uiMessage> {
        let mut raw: serde_json::Value = serde_json::from_str(json)?;
        if raw.get("version").and_then(|v| v.as_str()) == Some("1.0") {
            Self::validate_current_envelope(&raw)?;
        }
        if raw.get("version").and_then(|v| v.as_str()) == Some("v1.0") {
            // Only draft-version messages use this compatibility conversion.
            // Current messages must preserve ordinary path/call dictionaries.
            for pointer in [
                "/createSurface/components",
                "/updateComponents/components",
                "/callFunction",
            ] {
                if let Some(value) = raw.pointer_mut(pointer) {
                    // Event context values were dynamic in the draft too. Scope
                    // conversion to component actions, never arbitrary data maps.
                    if let Some(components) = value.as_array_mut() {
                        for component in components {
                            if let Some(context) = component.pointer_mut("/action/event/context") {
                                if let Some(entries) = context.as_object_mut() {
                                    for entry in entries.values_mut() {
                                        Self::normalize_draft_functions(entry, true);
                                    }
                                }
                            }
                        }
                    }
                    Self::normalize_draft_functions(value, false);
                }
            }
        }
        let msg: A2uiMessage = serde_json::from_value(raw)?;
        Ok(msg)
    }

    fn normalize_draft_functions(value: &mut serde_json::Value, in_args: bool) {
        match value {
            serde_json::Value::Object(object) => {
                let is_call = object
                    .get("call")
                    .or_else(|| object.get("@call"))
                    .is_some_and(serde_json::Value::is_string);
                if is_call {
                    object.remove("returnType");
                    object.remove("allowedCallers");
                    if let Some(call) = object.remove("call") {
                        object.insert("@call".into(), call);
                    }
                }
                if in_args
                    && object.len() == 1
                    && object.get("path").is_some_and(serde_json::Value::is_string)
                {
                    if let Some(path) = object.remove("path") {
                        object.insert("@path".into(), path);
                    }
                }
                for (key, child) in object {
                    Self::normalize_draft_functions(child, in_args || (is_call && key == "args"));
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    Self::normalize_draft_functions(child, in_args);
                }
            }
            _ => {}
        }
    }

    // Validate the wire rules that would otherwise be lost by serde's optional
    // fields / untagged envelopes. Legacy July draft messages remain accepted.
    fn validate_current_envelope(raw: &serde_json::Value) -> Result<()> {
        let obj = raw
            .as_object()
            .ok_or_else(|| A2uiError::Validation("message must be an object".into()))?;
        let payloads = [
            "createSurface",
            "updateComponents",
            "updateDataModel",
            "deleteSurface",
            "callRendererFunction",
            "agentFunctionResponse",
        ];
        if obj.len() != 2
            || payloads
                .iter()
                .filter(|key| obj.contains_key(**key))
                .count()
                != 1
        {
            return Err(A2uiError::Validation(
                "1.0 message requires version and exactly one current payload".into(),
            ));
        }
        if let Some(update) = obj.get("updateDataModel") {
            if update.get("value").is_none() {
                return Err(A2uiError::Validation(
                    "updateDataModel.value is required; use null to delete".into(),
                ));
            }
        }
        if let Some(create) = obj.get("createSurface") {
            if create.get("theme").is_some() || create.get("surfaceProperties").is_some() {
                return Err(A2uiError::Validation(
                    "1.0 createSurface does not accept theme or surfaceProperties".into(),
                ));
            }
            if create.get("dataModel").is_some_and(|v| !v.is_object()) {
                return Err(A2uiError::Validation(
                    "createSurface.dataModel must be an object".into(),
                ));
            }
        }
        for key in ["createSurface", "updateComponents"] {
            if let Some(components) = obj.get(key).and_then(|v| v.get("components")) {
                if !components.is_array() || components.as_array().is_some_and(Vec::is_empty) {
                    return Err(A2uiError::Validation(
                        "components must be a nonempty array".into(),
                    ));
                }
                if components
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.get("component").and_then(|v| v.as_str()) == Some("Surface"))
                {
                    return Err(A2uiError::Validation(
                        "Surface is reserved and cannot be sent as a component".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Collect current-protocol responses to agent calls for the host application.
    pub fn drain_agent_function_responses(&mut self) -> Vec<FunctionResponse> {
        std::mem::take(&mut self.agent_function_responses)
    }

    /// Queue an explicit renderer-to-agent RPC. Transport delivery belongs to the host.
    pub fn call_agent_function(&mut self, call: CallAgentFunction) -> Result<()> {
        if !self.agent_transport_available {
            return Err(A2uiError::InvalidFunctionCall(format!(
                "No outbound listener is available for agent function {}",
                call.call_function.call
            )));
        }
        if self.model.get_surface(&call.surface_id).is_none() {
            return Err(A2uiError::SurfaceNotFound(call.surface_id));
        }
        self.queue_outgoing(ClientMessage {
            version: "1.0".into(),
            payload: ClientPayload::CallAgentFunction(CallAgentFunctionPayload {
                call_agent_function: call,
            }),
        });
        Ok(())
    }

    /// Parse a JSONL stream (newline-delimited JSON objects).
    pub fn parse_jsonl(jsonl: &str) -> Vec<Result<A2uiMessage>> {
        jsonl
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(Self::parse_message)
            .collect()
    }

    /// Process a single parsed message.
    pub fn process_message(&mut self, msg: A2uiMessage) -> Result<()> {
        if msg.version != "1.0" && msg.version != "v1.0" {
            return Err(A2uiError::Validation(format!(
                "unsupported A2UI version: {}",
                msg.version
            )));
        }
        if msg.version == "1.0" {
            Self::validate_current_envelope(&serde_json::to_value(&msg)?)?;
        }
        // Hosts may deserialize or construct typed messages directly. Apply the
        // same idempotent draft conversion at the processing boundary too.
        let msg = if msg.version == "v1.0" {
            Self::parse_message(&serde_json::to_string(&msg)?)?
        } else {
            msg
        };
        let result = match &msg.payload {
            A2uiPayload::CallRendererFunction(payload) => {
                self.handle_renderer_function(&payload.call_renderer_function)
            }
            A2uiPayload::AgentFunctionResponse(payload) => {
                self.agent_function_responses
                    .push(payload.agent_function_response.clone());
                Ok(())
            }
            A2uiPayload::CreateSurface(payload) => {
                self.handle_create_surface(&payload.create_surface)?;
                if let Some(surface) = self
                    .model
                    .get_surface_mut(&payload.create_surface.surface_id)
                {
                    surface.current_protocol = msg.version == "1.0";
                }
                Ok(())
            }
            A2uiPayload::UpdateComponents(payload) => {
                self.handle_update_components(&payload.update_components)
            }
            A2uiPayload::UpdateDataModel(payload) => {
                self.handle_update_data_model(&payload.update_data_model)
            }
            A2uiPayload::DeleteSurface(payload) => {
                self.handle_delete_surface(&payload.delete_surface)
            }
            A2uiPayload::CallFunction(payload) => self.handle_call_function(payload),
            A2uiPayload::ActionResponse(payload) => self.handle_action_response(payload),
        };
        result?;
        match &msg.payload {
            A2uiPayload::AgentFunctionResponse(payload) => {
                let mut evaluator = std::mem::take(&mut self.evaluator);
                evaluator.response(self, &payload.agent_function_response);
                self.evaluator = evaluator;
            }
            A2uiPayload::UpdateDataModel(payload) => self
                .evaluator
                .invalidate_surface(&payload.update_data_model.surface_id, false),
            A2uiPayload::UpdateComponents(payload) => self
                .evaluator
                .invalidate_surface(&payload.update_components.surface_id, false),
            A2uiPayload::DeleteSurface(payload) => self
                .evaluator
                .invalidate_surface(&payload.delete_surface.surface_id, true),
            _ => {}
        }
        self.advance_evaluations();
        Ok(())
    }

    /// Process multiple messages sequentially.
    pub fn process_messages(&mut self, messages: Vec<A2uiMessage>) -> Vec<Result<()>> {
        messages
            .into_iter()
            .map(|m| self.process_message(m))
            .collect()
    }

    /// Drain outgoing client-to-server messages produced during processing.
    ///
    /// Call this after `process_message` / `process_messages` to retrieve
    /// any `functionResponse`, `error`, or other client messages that should
    /// be sent back to the server.
    pub fn drain_outgoing(&mut self) -> Vec<ClientMessage> {
        std::mem::take(&mut self.outgoing_messages)
    }

    /// Opt into payload validation (builder-style). When set, each
    /// `createSurface` / `updateComponents` payload is run through integrity +
    /// topology checks, and the findings accumulate in a report retrievable via
    /// [`Self::drain_validation`]. Validation never blocks loading — components
    /// are still added via graceful degradation.
    pub fn with_validation(mut self, cfg: crate::validate::ValidationConfig) -> Self {
        self.validation = Some(cfg);
        self
    }

    /// Drain the accumulated validation diagnostics from the last batch of
    /// processed messages. Returns an empty report when validation is off (the
    /// default) or when no problems were found.
    pub fn drain_validation(&mut self) -> crate::validate::ValidationReport {
        std::mem::take(&mut self.pending_validation)
    }

    /// Check if a component type exists in any registered catalog.
    pub fn catalog_type_exists(&self, component_type: &str) -> bool {
        self.catalogs
            .values()
            .any(|cat| cat.components.contains_key(component_type))
    }

    /// Return the IDs of all registered catalogs (native + inline).
    pub fn registered_catalog_ids(&self) -> Vec<String> {
        self.catalogs.keys().cloned().collect()
    }

    /// Register an inline catalog from a raw JSON value.
    ///
    /// The catalog is parsed via [`parse_inline_catalog`](crate::capabilities::parse_inline_catalog). Each
    /// declared function becomes a [`SchemaOnlyFunction`](crate::catalog::SchemaOnlyFunction) in a fresh
    /// [`Catalog`] (so `handle_call_function` can discover and reject
    /// execution attempts). Declared components have no native renderer and
    /// are *not* added to `catalog.components` — at render time they fall
    /// back to the generic renderer.
    pub fn register_inline_catalog(&mut self, json: serde_json::Value) -> Result<()> {
        let parsed = crate::capabilities::parse_inline_catalog(&json)?;

        let mut catalog = Catalog::new(parsed.catalog_id.clone());
        for func in &parsed.functions {
            let return_type = crate::catalog::schema_only::parse_return_type(&func.return_type);
            let declaration = &json["functions"][&func.name];
            use crate::catalog::function_api::AllowedCallers;
            let callers = match declaration
                .get("allowedCallers")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("rendererOnly")
            {
                "rendererOnly" => AllowedCallers::RendererOnly,
                "agentOnly" => AllowedCallers::AgentOnly,
                "rendererOrAgent" => AllowedCallers::RendererOrAgent,
                other => {
                    return Err(A2uiError::Validation(format!(
                        "unknown allowedCallers: {other}"
                    )));
                }
            };
            let activation = match declaration.get("requiresUserActivation") {
                None => false,
                Some(value) => value.as_bool().ok_or_else(|| {
                    A2uiError::Validation("requiresUserActivation must be boolean".into())
                })?,
            };
            let mut args_schema = declaration
                .pointer("/properties/args")
                .or_else(|| declaration.get("args"))
                .cloned()
                .unwrap_or(serde_json::json!({"type":"object"}));
            if let (Some(defs), Some(args)) = (json.get("$defs"), args_schema.as_object_mut()) {
                args.entry("$defs").or_insert_with(|| defs.clone());
            }
            let schema_func = crate::catalog::schema_only::SchemaOnlyFunction::new(
                func.name.clone(),
                return_type,
            )
            .with_policy(args_schema, callers, activation)?;
            catalog = catalog.with_function(Box::new(schema_func));
        }

        if let Some(version) = json
            .get("protocolVersion")
            .and_then(serde_json::Value::as_str)
        {
            self.catalog_versions.insert(
                parsed.catalog_id.clone(),
                version.trim_start_matches('v').into(),
            );
        }
        self.catalogs.insert(parsed.catalog_id, catalog);
        Ok(())
    }

    /// Register a pending action that expects a server response.
    ///
    /// Call this when the caller sends an `action` message with
    /// `wantResponse: true`. The `response_path` (if any) tells the
    /// processor where to store the server's response value in the data model.
    pub fn register_action(
        &mut self,
        surface_id: &str,
        action_id: &str,
        response_path: Option<String>,
    ) -> Result<()> {
        let surface = self
            .model
            .get_surface_mut(surface_id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(surface_id.to_string()))?;
        surface.pending_actions.borrow_mut().insert(
            action_id.to_string(),
            crate::model::surface_model::PendingAction {
                action_id: action_id.to_string(),
                response_path,
            },
        );
        Ok(())
    }

    /// Load a sample file (wrapping messages in {name, description, messages}).
    pub fn load_sample(json: &str) -> Result<(String, String, Vec<A2uiMessage>)> {
        let sample: serde_json::Value = serde_json::from_str(json)?;

        let name = sample["name"].as_str().unwrap_or("unnamed").to_string();
        let description = sample["description"].as_str().unwrap_or("").to_string();

        let messages_val = sample["messages"]
            .as_array()
            .ok_or_else(|| A2uiError::Validation("sample missing 'messages' array".into()))?;

        let messages: Vec<A2uiMessage> = messages_val
            .iter()
            .filter_map(|v| Self::parse_message(&v.to_string()).ok())
            .collect();

        Ok((name, description, messages))
    }

    // -----------------------------------------------------------------------
    // Message handlers
    // -----------------------------------------------------------------------

    fn handle_create_surface(&mut self, data: &CreateSurfaceData) -> Result<()> {
        // Validate surface doesn't already exist
        if self.model.get_surface(&data.surface_id).is_some() {
            return Err(A2uiError::SurfaceExists(data.surface_id.clone()));
        }

        let mut surface = SurfaceModel::new(
            data.surface_id.clone(),
            data.catalog_id.clone(),
            data.surface_properties.clone(),
            data.send_data_model,
        );

        surface.metadata = data.metadata.clone();

        // Initialize data model if provided
        if let Some(dm) = &data.data_model {
            surface = surface.with_data_model(dm.clone());
        }

        // Parse and add components if provided
        if let Some(components) = &data.components {
            surface.components.borrow_mut().add_from_json(components);
        }

        // Opt-in validation: run integrity + topology against the incoming raw
        // payload (NOT the internal model), accumulating diagnostics. This does
        // NOT change whether components were loaded above.
        if let Some(cfg) = self.validation
            && let Some(components) = &data.components
        {
            self.run_payload_validation(components, cfg);
        }

        self.model.add_surface(surface)
    }

    fn handle_update_components(&mut self, data: &UpdateComponentsData) -> Result<()> {
        // Graceful degradation: unknown component types are still added below
        // via add_from_json. We intentionally do NOT eprintln diagnostics here
        // — this is a library, and writing to stderr corrupts TUI consumers
        // (e.g. the gallery app renders into stderr).
        let surface = self
            .model
            .get_surface_mut(&data.surface_id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(data.surface_id.clone()))?;

        surface
            .components
            .borrow_mut()
            .add_from_json(&data.components);

        // Opt-in validation against the incoming raw payload.
        if let Some(cfg) = self.validation {
            self.run_payload_validation(&data.components, cfg);
        }

        Ok(())
    }

    fn handle_update_data_model(&mut self, data: &UpdateDataModelData) -> Result<()> {
        let surface = self
            .model
            .get_surface_mut(&data.surface_id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(data.surface_id.clone()))?;

        let path = data.path.as_deref().unwrap_or("/");
        let value = data.value.clone().unwrap_or(serde_json::Value::Null);

        surface
            .data_model
            .borrow_mut()
            .try_set(path, value)
            .map_err(A2uiError::InvalidPointer)?;
        Ok(())
    }

    fn handle_delete_surface(&mut self, data: &DeleteSurfaceData) -> Result<()> {
        self.model.delete_surface(&data.surface_id)
    }

    fn handle_renderer_function(&mut self, call: &CallRendererFunction) -> Result<()> {
        let result = self.execute_renderer_function(call, true);
        let outcome = match result {
            Ok(value) => FunctionOutcome::Success(FunctionValue { value }),
            Err(error) => FunctionOutcome::Failure(FunctionFailure {
                error: FunctionError {
                    code: "INVALID_FUNCTION_CALL".into(),
                    message: error.to_string(),
                },
            }),
        };
        self.queue_outgoing(ClientMessage {
            version: "1.0".into(),
            payload: ClientPayload::RendererFunctionResponse(RendererFunctionResponsePayload {
                renderer_function_response: FunctionResponse {
                    function_call_id: call.function_call_id.clone(),
                    outcome,
                },
            }),
        });
        Ok(())
    }

    fn execute_renderer_function(
        &self,
        call: &CallRendererFunction,
        current: bool,
    ) -> Result<serde_json::Value> {
        use crate::catalog::function_api::{AllowedCallers, ReturnType};
        let catalog_id = call.call_function.catalog_id.as_deref().ok_or_else(|| {
            A2uiError::Validation("callRendererFunction requires explicit catalogId".into())
        })?;
        let catalog = self
            .catalogs
            .get(catalog_id)
            .ok_or_else(|| A2uiError::Validation(format!("unknown catalog: {catalog_id}")))?;
        let expected_version = if current { "1.0" } else { "v1.0" };
        if self
            .catalog_versions
            .get(catalog_id)
            .is_some_and(|version| version != expected_version)
        {
            return Err(A2uiError::CatalogNotFound(catalog_id.into()));
        }
        let function = catalog
            .get_function(&call.call_function.call)
            .ok_or_else(|| {
                A2uiError::Validation(format!("unknown function: {}", call.call_function.call))
            })?;
        if function.allowed_callers() == AllowedCallers::RendererOnly
            || function.requires_user_activation()
        {
            return Err(A2uiError::Validation(
                "function is not callable by an agent".into(),
            ));
        }
        let empty = DataModel::new();
        let surface_data = match &call.surface_id {
            Some(id) => Some(
                self.model
                    .get_surface(id)
                    .ok_or_else(|| A2uiError::SurfaceNotFound(id.clone()))?
                    .data_model
                    .borrow(),
            ),
            None => None,
        };
        let data = surface_data.as_deref().unwrap_or(&empty);
        let default_catalog = call
            .surface_id
            .as_deref()
            .and_then(|id| self.model.get_surface(id))
            .map(|surface| surface.catalog_id.as_str());
        let mut context = crate::model::data_context::DataContext::new(data, &catalog.functions)
            .with_catalog_id(catalog_id)
            .with_catalogs(&self.catalogs, default_catalog)
            .with_catalog_versions(&self.catalog_versions)
            .with_agent_caller();
        if current {
            context = context.with_current_protocol();
        }
        let mut args: HashMap<_, _> = call
            .call_function
            .args
            .iter()
            .map(|(key, value)| {
                context
                    .resolve_dynamic_json(value)
                    .map(|value| (key.clone(), value))
            })
            .collect::<Result<_>>()?;
        function.prepare_args(&mut args)?;
        let value = function.execute(&args, &context)?;
        let valid = match function.return_type_for_protocol(current) {
            ReturnType::String => value.is_string(),
            ReturnType::Number => value.is_number(),
            ReturnType::Boolean => value.is_boolean(),
            ReturnType::Array => value.is_array(),
            ReturnType::Object => value.is_object(),
            ReturnType::Void => value.is_null(),
            ReturnType::Any => true,
        };
        if !valid {
            return Err(A2uiError::Validation(
                "function returned a value inconsistent with its declared return type".into(),
            ));
        }
        Ok(value)
    }

    fn handle_call_function(&mut self, payload: &CallFunctionPayload) -> Result<()> {
        let fc = &payload.call_function;
        let call_id = &payload.function_call_id;

        // Legacy messages still originate from an agent. Route them through the
        // same authorization, recursive argument evaluation and return checks as
        // current RPC, rather than treating them as local renderer evaluation.
        let execution_result = (|| {
            let mut call_function = fc.clone();
            if call_function.catalog_id.is_none() {
                let mut candidates = self
                    .catalogs
                    .iter()
                    .filter(|(_, catalog)| catalog.get_function(&fc.call).is_some());
                let (id, _) = candidates.next().ok_or_else(|| {
                    A2uiError::Validation(format!("function not found: {}", fc.call))
                })?;
                if candidates.next().is_some() {
                    return Err(A2uiError::Validation(
                        "legacy function call requires catalogId when the name is ambiguous".into(),
                    ));
                }
                call_function.catalog_id = Some(id.clone());
            }
            self.execute_renderer_function(
                &CallRendererFunction {
                    surface_id: None,
                    function_call_id: call_id.clone(),
                    call_function,
                },
                false,
            )
        })();

        // 6. Queue outgoing messages based on result
        match execution_result {
            Ok(result) => {
                if payload.want_response {
                    self.queue_outgoing(ClientMessage {
                        version: "v1.0".to_string(),
                        payload: ClientPayload::FunctionResponse(FunctionResponsePayload {
                            function_response: FunctionResponseData {
                                function_call_id: call_id.clone(),
                                call: fc.call.clone(),
                                value: result,
                            },
                        }),
                    });
                }
            }
            Err(e) => {
                self.queue_outgoing(ClientMessage {
                    version: "v1.0".to_string(),
                    payload: ClientPayload::Error(ErrorPayload {
                        error: ErrorData {
                            code: "INVALID_FUNCTION_CALL".to_string(),
                            message: e.to_string(),
                            surface_id: None,
                            function_call_id: Some(call_id.clone()),
                        },
                    }),
                });
            }
        }

        Ok(())
    }

    fn handle_action_response(
        &mut self,
        payload: &crate::protocol::server_to_client::ActionResponsePayload,
    ) -> Result<()> {
        let action_id = &payload.action_id;

        // Search all surfaces for the pending action
        for surface in self.model.surfaces_mut() {
            let pending = surface.pending_actions.borrow_mut().remove(action_id);
            if let Some(pa) = pending {
                if let Some(ref path) = pa.response_path
                    && let Some(ref value) = payload.action_response.value
                {
                    surface.data_model.borrow_mut().set(path, value.clone());
                }
                return Ok(());
            }
        }

        // No pending action found for this action_id — silently ignore
        // (the action may not have had wantResponse, or was already handled)
        Ok(())
    }

    /// Queue an outgoing client-to-server message.
    fn queue_outgoing(&mut self, msg: ClientMessage) {
        self.outgoing_messages.push(msg);
    }

    /// Run integrity + topology validation on a raw component payload slice and
    /// merge the findings into `pending_validation`. Borrows only the `&[Value]`
    /// data + a `Copy` config, so it is borrow-safe from within `&mut self`
    /// handlers.
    fn run_payload_validation(
        &mut self,
        components: &[serde_json::Value],
        cfg: crate::validate::ValidationConfig,
    ) {
        use crate::validate::{ROOT_ID, RefFieldSpec};

        let spec = RefFieldSpec::DEFAULT;
        let mut report = crate::validate::validate_component_integrity(
            components,
            &spec,
            ROOT_ID,
            cfg.allow_dangling_references,
            cfg.allow_missing_root,
        );
        let (_, topo) = crate::validate::analyze_topology(
            components,
            &spec,
            ROOT_ID,
            cfg.allow_orphan_components,
            cfg.allow_missing_root,
        );
        report.extend(topo);
        self.pending_validation.extend(report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_processor() -> MessageProcessor {
        MessageProcessor::new(vec![])
    }

    #[test]
    fn test_create_and_delete_surface() {
        let mut proc = make_processor();

        let msg = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "test_1",
                "catalogId": "https://example.com/catalog.json"
            }
        });
        let parsed = MessageProcessor::parse_message(&msg.to_string()).unwrap();
        proc.process_message(parsed).unwrap();

        assert!(proc.model.get_surface("test_1").is_some());

        let del = serde_json::json!({
            "version": "v1.0",
            "deleteSurface": {
                "surfaceId": "test_1"
            }
        });
        let parsed = MessageProcessor::parse_message(&del.to_string()).unwrap();
        proc.process_message(parsed).unwrap();

        assert!(proc.model.get_surface("test_1").is_none());
    }

    #[test]
    fn test_update_components() {
        let mut proc = make_processor();

        // Create surface
        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "s1",
                "catalogId": "test"
            }
        });
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        // Update components
        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": {
                "surfaceId": "s1",
                "components": [
                    {"id": "root", "component": "Column", "children": ["hello"]},
                    {"id": "hello", "component": "Text", "text": "Hello World"}
                ]
            }
        });
        proc.process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();

        let surface = proc.model.get_surface("s1").unwrap();
        let components = surface.components.borrow();
        assert!(components.contains("root"));
        assert!(components.contains("hello"));
        assert_eq!(components.get("hello").unwrap().component_type, "Text");
    }

    #[test]
    fn test_update_data_model() {
        let mut proc = make_processor();

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "s1",
                "catalogId": "test",
                "dataModel": {"name": "Alice"}
            }
        });
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        // Update a field
        let update = serde_json::json!({
            "version": "v1.0",
            "updateDataModel": {
                "surfaceId": "s1",
                "path": "/name",
                "value": "Bob"
            }
        });
        proc.process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();

        let surface = proc.model.get_surface("s1").unwrap();
        assert_eq!(
            surface.data_model.borrow().get("/name"),
            Some(&serde_json::json!("Bob"))
        );
    }

    #[test]
    fn test_duplicate_surface_error() {
        let mut proc = make_processor();

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "dup",
                "catalogId": "test"
            }
        });
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        let result =
            proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_jsonl() {
        let jsonl = r#"
{"version":"v1.0","createSurface":{"surfaceId":"main","catalogId":"test"}}
{"version":"v1.0","updateComponents":{"surfaceId":"main","components":[{"id":"root","component":"Text","text":"Hi"}]}}
"#;
        let messages = MessageProcessor::parse_jsonl(jsonl);
        assert_eq!(messages.len(), 2);
        assert!(messages[0].is_ok());
        assert!(messages[1].is_ok());
    }

    #[test]
    fn test_spec_simple_text_sample() {
        let sample = r#"{
            "name": "Simple Text",
            "description": "Basic text rendering",
            "messages": [
                {
                    "version": "v1.0",
                    "createSurface": {
                        "surfaceId": "example_1",
                        "catalogId": "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json"
                    }
                },
                {
                    "version": "v1.0",
                    "updateComponents": {
                        "surfaceId": "example_1",
                        "components": [
                            {"id": "root", "component": "Text", "text": "Hello, Minimal Catalog!", "variant": "h1"}
                        ]
                    }
                }
            ]
        }"#;

        let (name, _desc, messages) = MessageProcessor::load_sample(sample).unwrap();
        assert_eq!(name, "Simple Text");
        assert_eq!(messages.len(), 2);

        let mut proc = make_processor();
        let results = proc.process_messages(messages);
        assert!(results.iter().all(|r| r.is_ok()));

        let surface = proc.model.get_surface("example_1").unwrap();
        let components = surface.components.borrow();
        let root = components.get("root").unwrap();
        assert_eq!(root.component_type, "Text");
        assert_eq!(root.get_raw("text").unwrap(), "Hello, Minimal Catalog!");
        assert_eq!(root.get_raw("variant").unwrap(), "h1");
    }

    #[test]
    fn test_spec_login_form_sample() {
        let sample = r#"{
            "name": "Login Form",
            "description": "Form with input fields and action",
            "messages": [
                {
                    "version": "v1.0",
                    "createSurface": {
                        "surfaceId": "example_4",
                        "catalogId": "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json",
                        "sendDataModel": true
                    }
                },
                {
                    "version": "v1.0",
                    "updateComponents": {
                        "surfaceId": "example_4",
                        "components": [
                            {"id": "root", "component": "Column", "children": ["form_title", "username_field", "password_field", "submit_button"], "justify": "start", "align": "stretch"},
                            {"id": "form_title", "component": "Text", "text": "Login", "variant": "h2"},
                            {"id": "username_field", "component": "TextField", "label": "Username", "value": {"path": "/username"}, "variant": "shortText"},
                            {"id": "password_field", "component": "TextField", "label": "Password", "value": {"path": "/password"}, "variant": "obscured"},
                            {"id": "submit_button", "component": "Button", "child": "submit_label", "variant": "primary", "action": {"event": {"name": "login_submitted", "context": {"user": {"path": "/username"}, "pass": {"path": "/password"}}}}},
                            {"id": "submit_label", "component": "Text", "text": "Sign In"}
                        ]
                    }
                }
            ]
        }"#;

        let (_name, _desc, messages) = MessageProcessor::load_sample(sample).unwrap();
        assert_eq!(messages.len(), 2);

        let mut proc = make_processor();
        let results = proc.process_messages(messages);
        assert!(results.iter().all(|r| r.is_ok()));

        let surface = proc.model.get_surface("example_4").unwrap();
        assert!(surface.send_data_model);

        let components = surface.components.borrow();
        assert_eq!(components.len(), 6);

        let root = components.get("root").unwrap();
        assert_eq!(root.component_type, "Column");
        let children = root.children().unwrap();
        match children {
            crate::protocol::common_types::ChildList::Static(ids) => {
                assert_eq!(ids.len(), 4);
                assert_eq!(ids[0], "form_title");
            }
            _ => panic!("expected static children"),
        }

        let submit = components.get("submit_button").unwrap();
        assert_eq!(submit.component_type, "Button");
        assert!(submit.action().is_some());
    }

    #[test]
    fn test_validation_hook_reports_and_still_loads() {
        // With STRICT validation enabled: a dangling child ref should produce a
        // non-empty report, AND the component must still be loaded (graceful
        // degradation is unchanged).
        let mut proc =
            MessageProcessor::new(vec![]).with_validation(crate::validate::STRICT_VALIDATION);

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "s1",
                "catalogId": "test",
                "components": [
                    {"id": "root", "component": "Column", "children": ["ghost"]}
                ]
            }
        });
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        // Surface + root component were loaded despite the dangling ref.
        assert!(proc.model.get_surface("s1").is_some());
        let surface = proc.model.get_surface("s1").unwrap();
        assert!(surface.components.borrow().contains("root"));

        // The dangling reference is reported.
        let report = proc.drain_validation();
        assert!(!report.is_empty());
        assert!(report.has_code(&crate::validate::ValidationErrorCode::DanglingReference));

        // A follow-up updateComponents with another dangling ref accumulates.
        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": {
                "surfaceId": "s1",
                "components": [
                    {"id": "root2", "component": "Column", "child": "also_ghost"}
                ]
            }
        });
        proc.process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();
        let report2 = proc.drain_validation();
        assert!(!report2.is_empty());
    }

    #[test]
    fn test_default_processor_has_empty_validation() {
        // Default (no with_validation): drain_validation is always empty.
        let mut proc = MessageProcessor::new(vec![]);

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "s1",
                "catalogId": "test",
                "components": [
                    {"id": "root", "component": "Column", "children": ["ghost"]}
                ]
            }
        });
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        // Component still loaded (graceful degradation).
        assert!(proc.model.get_surface("s1").is_some());
        assert!(
            proc.model
                .get_surface("s1")
                .unwrap()
                .components
                .borrow()
                .contains("root")
        );

        // No validation report produced.
        assert!(proc.drain_validation().is_empty());
    }
}
