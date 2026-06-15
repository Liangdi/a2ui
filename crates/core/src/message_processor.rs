//! Message processor — parses A2UI messages and mutates state models.

use std::collections::HashMap;

use crate::catalog::Catalog;
use crate::error::{A2uiError, Result};
use crate::model::data_model::DataModel;
use crate::model::surface_model::SurfaceModel;
use crate::model::surface_group_model::SurfaceGroupModel;
use crate::protocol::client_to_server::{
    ClientMessage, ClientPayload, ErrorData, ErrorPayload, FunctionResponseData,
    FunctionResponsePayload,
};
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
    /// Outgoing client-to-server messages produced during processing.
    outgoing_messages: Vec<ClientMessage>,
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
        let catalog_map: HashMap<String, Catalog> = catalogs
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        Self {
            model: SurfaceGroupModel::new(),
            catalogs: catalog_map,
            outgoing_messages: Vec::new(),
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
        self.pending_validation = crate::validate::ValidationReport::new();
    }

    /// Parse a raw JSON string into an A2uiMessage.
    pub fn parse_message(json: &str) -> Result<A2uiMessage> {
        let msg: A2uiMessage = serde_json::from_str(json)?;
        Ok(msg)
    }

    /// Parse a JSONL stream (newline-delimited JSON objects).
    pub fn parse_jsonl(jsonl: &str) -> Vec<Result<A2uiMessage>> {
        jsonl.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| Self::parse_message(line))
            .collect()
    }

    /// Process a single parsed message.
    pub fn process_message(&mut self, msg: A2uiMessage) -> Result<()> {
        match &msg.payload {
            A2uiPayload::CreateSurface(payload) => {
                self.handle_create_surface(&payload.create_surface)
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
            A2uiPayload::CallFunction(payload) => {
                self.handle_call_function(payload)
            }
            A2uiPayload::ActionResponse(payload) => {
                self.handle_action_response(payload)
            }
        }
    }

    /// Process multiple messages sequentially.
    pub fn process_messages(&mut self, messages: Vec<A2uiMessage>) -> Vec<Result<()>> {
        messages.into_iter().map(|m| self.process_message(m)).collect()
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
        self.catalogs.values()
            .any(|cat| cat.components.contains_key(component_type))
    }

    /// Return the IDs of all registered catalogs (native + inline).
    pub fn registered_catalog_ids(&self) -> Vec<String> {
        self.catalogs.keys().cloned().collect()
    }

    /// Register an inline catalog from a raw JSON value.
    ///
    /// The catalog is parsed via [`capabilities::parse_inline_catalog`]. Each
    /// declared function becomes a [`SchemaOnlyFunction`] in a fresh
    /// [`Catalog`] (so `handle_call_function` can discover and reject
    /// execution attempts). Declared components have no native renderer and
    /// are *not* added to `catalog.components` — at render time they fall
    /// back to the generic renderer.
    pub fn register_inline_catalog(&mut self, json: serde_json::Value) -> Result<()> {
        let parsed = crate::capabilities::parse_inline_catalog(&json)?;

        let mut catalog = Catalog::new(parsed.catalog_id.clone());
        for func in &parsed.functions {
            let return_type = crate::catalog::schema_only::parse_return_type(&func.return_type);
            let schema_func = crate::catalog::schema_only::SchemaOnlyFunction::new(
                func.name.clone(),
                return_type,
            );
            catalog = catalog.with_function(Box::new(schema_func));
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
        surface
            .pending_actions
            .borrow_mut()
            .insert(
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

        let messages_val = sample["messages"].as_array().ok_or_else(|| {
            A2uiError::Validation("sample missing 'messages' array".into())
        })?;

        let messages: Vec<A2uiMessage> = messages_val
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
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
        if let Some(cfg) = self.validation {
            if let Some(components) = &data.components {
                self.run_payload_validation(components, cfg);
            }
        }

        self.model.add_surface(surface)
    }

    fn handle_update_components(&mut self, data: &UpdateComponentsData) -> Result<()> {
        // Graceful degradation: unknown component types are still added below
        // via add_from_json. We intentionally do NOT eprintln diagnostics here
        // — this is a library, and writing to stderr corrupts TUI consumers
        // (e.g. the gallery app renders into stderr).
        let surface = self.model.get_surface_mut(&data.surface_id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(data.surface_id.clone()))?;

        surface.components.borrow_mut().add_from_json(&data.components);

        // Opt-in validation against the incoming raw payload.
        if let Some(cfg) = self.validation {
            self.run_payload_validation(&data.components, cfg);
        }

        Ok(())
    }

    fn handle_update_data_model(&mut self, data: &UpdateDataModelData) -> Result<()> {
        let surface = self.model.get_surface_mut(&data.surface_id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(data.surface_id.clone()))?;

        let path = data.path.as_deref().unwrap_or("/");
        let value = data.value.clone().unwrap_or(serde_json::Value::Null);

        if path == "/" || path.is_empty() {
            if value.is_null() {
                surface.data_model.borrow_mut().replace_all(serde_json::json!({}));
            } else {
                surface.data_model.borrow_mut().replace_all(value);
            }
        } else {
            surface.data_model.borrow_mut().set(path, value);
        }
        Ok(())
    }

    fn handle_delete_surface(&mut self, data: &DeleteSurfaceData) -> Result<()> {
        self.model.delete_surface(&data.surface_id)
    }

    fn handle_call_function(&mut self, payload: &CallFunctionPayload) -> Result<()> {
        let fc = &payload.call_function;
        let call_id = &payload.function_call_id;

        // 1. Find the function across all catalogs
        let mut found_func: Option<&dyn crate::catalog::function_api::FunctionImplementation> = None;
        let mut found_functions_map: Option<&std::collections::HashMap<String, Box<dyn crate::catalog::function_api::FunctionImplementation>>> = None;

        for catalog in self.catalogs.values() {
            if let Some(f) = catalog.get_function(&fc.call) {
                found_func = Some(f);
                found_functions_map = Some(&catalog.functions);
                break;
            }
        }

        // 2. Function not found → reject with error
        let Some(func) = found_func else {
            self.queue_outgoing(ClientMessage {
                version: "v1.0".to_string(),
                payload: ClientPayload::Error(ErrorPayload {
                    error: ErrorData {
                        code: "INVALID_FUNCTION_CALL".to_string(),
                        message: format!("function not found: {}", fc.call),
                        surface_id: None,
                        function_call_id: Some(call_id.clone()),
                    },
                }),
            });
            return Ok(());
        };

        // 3. Build a DataContext using the first available surface's DataModel.
        //    We execute and collect results in a block so the borrows are dropped
        //    before we call queue_outgoing (which needs &mut self).
        let execution_result: std::result::Result<serde_json::Value, A2uiError> = {
            let empty_dm;
            let data_model: &DataModel = match self.model.surfaces().next() {
                Some(surface) => &surface.data_model.borrow(),
                None => {
                    empty_dm = DataModel::new();
                    &empty_dm
                }
            };
            let functions_map = found_functions_map.unwrap();
            let ctx = crate::model::data_context::DataContext::new(data_model, functions_map);

            // 4. Resolve args (may contain path bindings or nested function calls)
            let mut resolved_args = HashMap::new();
            for (key, val) in &fc.args {
                let resolved = ctx.resolve_dynamic_value(
                    &serde_json::from_value::<crate::protocol::common_types::DynamicValue>(val.clone())
                        .unwrap_or(crate::protocol::common_types::DynamicValue::String(val.to_string())),
                );
                resolved_args.insert(key.clone(), resolved);
            }

            // 5. Execute the function
            func.execute(&resolved_args, &ctx)
        };
        // borrows on self.model and self.catalogs are released here

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
                if let Some(ref path) = pa.response_path {
                    if let Some(ref value) = payload.action_response.value {
                        surface.data_model.borrow_mut().set(path, value.clone());
                    }
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
        use crate::validate::{RefFieldSpec, ROOT_ID};

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
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap()).unwrap();

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
        proc.process_message(MessageProcessor::parse_message(&update.to_string()).unwrap()).unwrap();

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
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap()).unwrap();

        // Update a field
        let update = serde_json::json!({
            "version": "v1.0",
            "updateDataModel": {
                "surfaceId": "s1",
                "path": "/name",
                "value": "Bob"
            }
        });
        proc.process_message(MessageProcessor::parse_message(&update.to_string()).unwrap()).unwrap();

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
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap()).unwrap();

        let result = proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap());
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

        let (name, desc, messages) = MessageProcessor::load_sample(sample).unwrap();
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
        assert!(proc
            .model
            .get_surface("s1")
            .unwrap()
            .components
            .borrow()
            .contains("root"));

        // No validation report produced.
        assert!(proc.drain_validation().is_empty());
    }

}
