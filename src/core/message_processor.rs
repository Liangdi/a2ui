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

    // ===================================================================
    // Basic catalog end-to-end tests
    // ===================================================================

    /// Helper: build a MessageProcessor with both minimal and basic catalogs.
    fn make_basic_processor() -> MessageProcessor {
        use crate::tui::catalogs::basic::build_basic_catalog;
        use crate::tui::catalogs::minimal::build_minimal_catalog;
        MessageProcessor::new(vec![build_minimal_catalog(), build_basic_catalog()])
    }

    #[test]
    fn test_e2e_all_basic_samples_load_and_parse() {
        use crate::gallery::sample_loader;

        let samples = sample_loader::load_samples_from_dir(
            "/home/liangdi/workspace/ai/a2ui/specification/v1_0/catalogs/basic/examples",
        );

        assert!(
            samples.len() >= 30,
            "should load at least 30 basic samples, got {}",
            samples.len()
        );

        // Each sample should have at least one message
        for sample in &samples {
            assert!(
                !sample.messages.is_empty(),
                "sample '{}' has no messages",
                sample.name
            );
        }

        // All should process without errors using both catalogs
        for sample in &samples {
            let mut proc = make_basic_processor();
            let results = proc.process_messages(sample.messages.clone());
            for r in &results {
                assert!(
                    r.is_ok(),
                    "sample '{}' failed: {:?}",
                    sample.name,
                    r
                );
            }
        }
    }

    #[test]
    fn test_e2e_contact_form_sample() {
        // Build the JSONL programmatically to avoid raw-string / JSON escape conflicts.
        let msg_create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "contact_form_1",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
            }
        });

        let msg_update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": {
                "surfaceId": "contact_form_1",
                "components": [
                    {"id":"root","component":"Card","child":"form_container"},
                    {"id":"form_container","component":"Column","children":["header_row","name_row","email_group","phone_group","pref_group","divider_1","newsletter_checkbox","submit_button"],"justify":"start","align":"stretch"},
                    {"id":"header_row","component":"Row","children":["header_icon","header_text"],"align":"center"},
                    {"id":"header_icon","component":"Icon","name":"mail"},
                    {"id":"header_text","component":"Text","text":"# Contact Us","variant":"h2"},
                    {"id":"name_row","component":"Row","children":["first_name_group","last_name_group"],"justify":"spaceBetween"},
                    {"id":"first_name_group","component":"Column","children":["first_name_label","first_name_field"],"weight":1},
                    {"id":"first_name_label","component":"Text","text":"First Name","variant":"caption"},
                    {"id":"first_name_field","component":"TextField","label":"First Name","value":{"path":"/contact/firstName"},"variant":"shortText"},
                    {"id":"last_name_group","component":"Column","children":["last_name_label","last_name_field"],"weight":1},
                    {"id":"last_name_label","component":"Text","text":"Last Name","variant":"caption"},
                    {"id":"last_name_field","component":"TextField","label":"Last Name","value":{"path":"/contact/lastName"},"variant":"shortText"},
                    {"id":"email_group","component":"Column","children":["email_label","email_field"]},
                    {"id":"email_label","component":"Text","text":"Email Address","variant":"caption"},
                    {"id":"email_field","component":"TextField","label":"Email","value":{"path":"/contact/email"},"variant":"shortText","checks":[{"call":"required","args":{"value":{"path":"/contact/email"}},"message":"Email is required."},{"call":"email","args":{"value":{"path":"/contact/email"}},"message":"Please enter a valid email address."}]},
                    {"id":"phone_group","component":"Column","children":["phone_label","phone_field"]},
                    {"id":"phone_label","component":"Text","text":"Phone Number","variant":"caption"},
                    {"id":"phone_field","component":"TextField","label":"Phone","value":{"path":"/contact/phone"},"variant":"shortText","checks":[{"call":"regex","args":{"value":{"path":"/contact/phone"},"pattern":"^\\d{10}$"},"message":"Phone number must be 10 digits."}]},
                    {"id":"pref_group","component":"Column","children":["pref_label","pref_picker"]},
                    {"id":"pref_label","component":"Text","text":"Preferred Contact Method","variant":"caption"},
                    {"id":"pref_picker","component":"ChoicePicker","variant":"mutuallyExclusive","options":[{"label":"Email","value":"email"},{"label":"Phone","value":"phone"},{"label":"SMS","value":"sms"}],"value":{"path":"/contact/preference"}},
                    {"id":"divider_1","component":"Divider","axis":"horizontal"},
                    {"id":"newsletter_checkbox","component":"CheckBox","label":"Subscribe to our newsletter","value":{"path":"/contact/subscribe"}},
                    {"id":"submit_button_label","component":"Text","text":"Send Message"},
                    {"id":"submit_button","component":"Button","child":"submit_button_label","variant":"primary","action":{"event":{"name":"submitContactForm","context":{"formId":"contact_form_1"}}}}
                ]
            }
        });

        let msg_data = serde_json::json!({
            "version": "v1.0",
            "updateDataModel": {
                "surfaceId": "contact_form_1",
                "path": "/contact",
                "value": {"firstName":"John","lastName":"Doe","email":"john.doe@example.com","phone":"1234567890","preference":["email"],"subscribe":true}
            }
        });

        let msg_delete = serde_json::json!({
            "version": "v1.0",
            "deleteSurface": {"surfaceId":"contact_form_1"}
        });

        // Build JSONL and parse
        let jsonl = format!(
            "{}\n{}\n{}\n{}\n",
            msg_create, msg_update, msg_data, msg_delete
        );
        let parsed = MessageProcessor::parse_jsonl(&jsonl);
        assert_eq!(parsed.len(), 4, "should have 4 messages in JSONL stream");
        for (i, msg) in parsed.iter().enumerate() {
            assert!(msg.is_ok(), "message {} failed to parse: {:?}", i, msg);
        }

        let messages: Vec<A2uiMessage> = parsed.into_iter().map(|r| r.unwrap()).collect();

        let mut proc = make_basic_processor();
        let results = proc.process_messages(messages);
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_ok(), "message {} failed to process: {:?}", i, r);
        }

        // After deleteSurface the surface should be gone
        assert!(
            proc.model.get_surface("contact_form_1").is_none(),
            "surface should be deleted"
        );

        // Now process again WITHOUT the delete to verify the tree and data model.
        let jsonl_no_delete = format!("{}\n{}\n{}\n", msg_create, msg_update, msg_data);
        let parsed2 = MessageProcessor::parse_jsonl(&jsonl_no_delete);
        let messages2: Vec<A2uiMessage> = parsed2.into_iter().map(|r| r.unwrap()).collect();

        let mut proc2 = make_basic_processor();
        let results2 = proc2.process_messages(messages2);
        for (i, r) in results2.iter().enumerate() {
            assert!(r.is_ok(), "message {} failed to process: {:?}", i, r);
        }

        // Verify the component tree
        let surface = proc2.model.get_surface("contact_form_1").unwrap();
        assert!(surface.has_root());

        let components = surface.components.borrow();

        // root is Card
        let root = components.get("root").unwrap();
        assert_eq!(root.component_type, "Card");

        // form_container is Column with 8 children
        let form_container = components.get("form_container").unwrap();
        assert_eq!(form_container.component_type, "Column");
        let children = form_container.children().unwrap();
        match children {
            crate::core::protocol::common_types::ChildList::Static(ids) => {
                assert_eq!(ids.len(), 8, "form_container should have 8 children");
            }
            _ => panic!("expected static children for form_container"),
        }

        // Verify data model has contact data
        let dm = surface.data_model.borrow();
        assert_eq!(
            dm.get("/contact/firstName"),
            Some(&serde_json::json!("John"))
        );
        assert_eq!(
            dm.get("/contact/lastName"),
            Some(&serde_json::json!("Doe"))
        );
        assert_eq!(
            dm.get("/contact/email"),
            Some(&serde_json::json!("john.doe@example.com"))
        );
        assert_eq!(
            dm.get("/contact/phone"),
            Some(&serde_json::json!("1234567890"))
        );
        assert_eq!(
            dm.get("/contact/subscribe"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn test_e2e_basic_catalog_functions_in_context() {
        use crate::core::catalog::basic_functions::{
            EmailFunction, FormatStringFunction, RequiredFunction,
        };
        use crate::core::catalog::function_api::FunctionImplementation;
        use crate::core::model::data_context::DataContext;
        use crate::core::model::data_model::DataModel;
        use crate::core::protocol::common_types::{DynamicBoolean, DynamicString, FunctionCall};

        // Build the function map from the basic catalog
        let func_map: std::collections::HashMap<
            String,
            Box<dyn crate::core::catalog::function_api::FunctionImplementation>,
        > = {
            let mut m = std::collections::HashMap::new();
            let req: Box<dyn FunctionImplementation> = Box::new(RequiredFunction);
            m.insert(req.name().to_string(), req);
            let email: Box<dyn FunctionImplementation> = Box::new(EmailFunction);
            m.insert(email.name().to_string(), email);
            let fmt: Box<dyn FunctionImplementation> = Box::new(FormatStringFunction);
            m.insert(fmt.name().to_string(), fmt);
            m
        };

        // Create a data model with test data
        let dm = DataModel::from_value(serde_json::json!({
            "contact": {
                "email": "user@example.com",
                "name": "Alice"
            }
        }));

        let ctx = DataContext::new(&dm, &func_map);

        // --- Test formatString via DataContext.resolve_dynamic_string ---
        let format_str = DynamicString::Function(FunctionCall {
            call: "formatString".to_string(),
            args: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "value".to_string(),
                    serde_json::json!("Hello, ${/contact/name}!"),
                );
                m
            },
        });
        let result = ctx.resolve_dynamic_string(&format_str);
        assert_eq!(result, "Hello, Alice!");

        // --- Test required via DynamicBoolean resolution ---
        // "some text" is present => true
        let required_present = DynamicBoolean::Function(FunctionCall {
            call: "required".to_string(),
            args: {
                let mut m = std::collections::HashMap::new();
                m.insert("value".to_string(), serde_json::json!("some text"));
                m
            },
        });
        assert!(ctx.resolve_dynamic_boolean(&required_present));

        // empty string => false
        let required_empty = DynamicBoolean::Function(FunctionCall {
            call: "required".to_string(),
            args: {
                let mut m = std::collections::HashMap::new();
                m.insert("value".to_string(), serde_json::json!(""));
                m
            },
        });
        assert!(!ctx.resolve_dynamic_boolean(&required_empty));

        // --- Test email via DynamicBoolean resolution ---
        let email_valid = DynamicBoolean::Function(FunctionCall {
            call: "email".to_string(),
            args: {
                let mut m = std::collections::HashMap::new();
                m.insert("value".to_string(), serde_json::json!("user@example.com"));
                m
            },
        });
        assert!(ctx.resolve_dynamic_boolean(&email_valid));

        let email_bad = DynamicBoolean::Function(FunctionCall {
            call: "email".to_string(),
            args: {
                let mut m = std::collections::HashMap::new();
                m.insert("value".to_string(), serde_json::json!("not-an-email"));
                m
            },
        });
        assert!(!ctx.resolve_dynamic_boolean(&email_bad));

        // Also verify via full MessageProcessor pipeline: create surface, set data, process messages
        let mut proc = make_basic_processor();

        // Create surface
        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "func_test",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "sendDataModel": true
            }
        });
        proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        // Set data model
        let update_data = serde_json::json!({
            "version": "v1.0",
            "updateDataModel": {
                "surfaceId": "func_test",
                "path": "/",
                "value": {"email": "test@test.com", "name": "Bob"}
            }
        });
        proc.process_message(MessageProcessor::parse_message(&update_data.to_string()).unwrap())
            .unwrap();

        // Verify data was set
        let surface = proc.model.get_surface("func_test").unwrap();
        assert_eq!(
            surface.data_model.borrow().get("/email"),
            Some(&serde_json::json!("test@test.com"))
        );
        assert_eq!(
            surface.data_model.borrow().get("/name"),
            Some(&serde_json::json!("Bob"))
        );
    }
}
