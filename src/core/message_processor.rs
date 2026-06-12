//! Message processor — parses A2UI messages and mutates state models.

use std::collections::HashMap;

use crate::core::catalog::Catalog;
use crate::core::error::{A2uiError, Result};
use crate::core::model::surface_model::SurfaceModel;
use crate::core::model::surface_group_model::SurfaceGroupModel;
use crate::core::protocol::server_to_client::{
    A2uiMessage, A2uiPayload, CreateSurfaceData, DeleteSurfaceData, UpdateComponentsData,
    UpdateDataModelData,
};

/// Parses A2UI JSON messages and applies them to the state models.
pub struct MessageProcessor {
    /// The state model (all active surfaces).
    pub model: SurfaceGroupModel,
    /// Registered catalogs keyed by catalog ID.
    #[allow(dead_code)]
    catalogs: HashMap<String, Catalog>,
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
        }
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
            A2uiPayload::CallFunction(_) => {
                // TODO: handle callFunction
                Ok(())
            }
            A2uiPayload::ActionResponse(_) => {
                // TODO: handle actionResponse
                Ok(())
            }
        }
    }

    /// Process multiple messages sequentially.
    pub fn process_messages(&mut self, messages: Vec<A2uiMessage>) -> Vec<Result<()>> {
        messages.into_iter().map(|m| self.process_message(m)).collect()
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

        self.model.add_surface(surface)
    }

    fn handle_update_components(&mut self, data: &UpdateComponentsData) -> Result<()> {
        let surface = self.model.get_surface_mut(&data.surface_id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(data.surface_id.clone()))?;

        surface.components.borrow_mut().add_from_json(&data.components);
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
            crate::core::protocol::common_types::ChildList::Static(ids) => {
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
    fn test_e2e_simple_text_render_pipeline() {
        // Full pipeline: load sample → process messages → verify component tree
        use crate::tui::catalogs::minimal::{build_minimal_catalog, build_minimal_registry};

        let catalog = build_minimal_catalog();
        let _registry = build_minimal_registry();

        // Load the actual spec example
        let spec_path = "/home/liangdi/workspace/ai/a2ui/specification/v1_0/catalogs/minimal/examples/1_simple_text.json";
        let content = std::fs::read_to_string(spec_path).expect("spec file should exist");
        let (_name, _desc, messages) = MessageProcessor::load_sample(&content).unwrap();

        let mut proc = MessageProcessor::new(vec![]);
        let results = proc.process_messages(messages);
        for r in &results {
            assert!(r.is_ok(), "message processing failed: {:?}", r);
        }

        // Verify the surface and component tree
        let surface = proc.model.get_surface("example_1").unwrap();
        assert!(surface.has_root());

        let components = surface.components.borrow();
        let root = components.get("root").unwrap();
        assert_eq!(root.component_type, "Text");
        assert_eq!(root.get_raw("text").unwrap(), "Hello, Minimal Catalog!");
        assert_eq!(root.get_raw("variant").unwrap(), "h1");
    }

    #[test]
    fn test_e2e_login_form_render_pipeline() {
        use crate::tui::catalogs::minimal::{build_minimal_catalog, build_minimal_registry};

        let _catalog = build_minimal_catalog();
        let _registry = build_minimal_registry();

        let spec_path = "/home/liangdi/workspace/ai/a2ui/specification/v1_0/catalogs/minimal/examples/4_login_form.json";
        let content = std::fs::read_to_string(spec_path).expect("spec file should exist");
        let (_name, _desc, messages) = MessageProcessor::load_sample(&content).unwrap();

        let mut proc = MessageProcessor::new(vec![]);
        let results = proc.process_messages(messages);
        for r in &results {
            assert!(r.is_ok(), "message processing failed: {:?}", r);
        }

        let surface = proc.model.get_surface("example_4").unwrap();
        assert!(surface.send_data_model);

        let components = surface.components.borrow();
        assert_eq!(components.len(), 6);

        // Verify the tree structure: root → Column with 4 children
        let root = components.get("root").unwrap();
        assert_eq!(root.component_type, "Column");
        let children = root.children().unwrap();
        match children {
            crate::core::protocol::common_types::ChildList::Static(ids) => {
                assert_eq!(ids, vec!["form_title", "username_field", "password_field", "submit_button"]);
            }
            _ => panic!("expected static children"),
        }

        // Verify Button → Text child
        let submit = components.get("submit_button").unwrap();
        assert_eq!(submit.child(), Some("submit_label".to_string()));
        let label = components.get("submit_label").unwrap();
        assert_eq!(label.get_raw("text").unwrap(), "Sign In");

        // Verify TextField has dynamic binding
        let username = components.get("username_field").unwrap();
        let value_binding: crate::core::protocol::common_types::DynamicString =
            username.get_property("value").unwrap();
        match value_binding {
            crate::core::protocol::common_types::DynamicString::Binding(b) => {
                assert_eq!(b.path, "/username");
            }
            _ => panic!("expected binding for username value"),
        }
    }

    #[test]
    fn test_e2e_all_minimal_samples_load_and_parse() {
        use crate::gallery::sample_loader;

        let samples = sample_loader::load_samples_from_dir(
            "/home/liangdi/workspace/ai/a2ui/specification/v1_0/catalogs/minimal/examples",
        );

        assert!(samples.len() >= 7, "should load at least 7 minimal samples, got {}", samples.len());

        // Each sample should have at least one message
        for sample in &samples {
            assert!(!sample.messages.is_empty(), "sample '{}' has no messages", sample.name);
        }

        // All should process without errors
        for sample in &samples {
            let mut proc = MessageProcessor::new(vec![]);
            let results = proc.process_messages(sample.messages.clone());
            for r in &results {
                assert!(r.is_ok(), "sample '{}' failed: {:?}", sample.name, r);
            }
        }
    }
}
