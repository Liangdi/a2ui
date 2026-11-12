//! Explicit user-action entry point. Passive expression evaluation never enters here.
use super::MessageProcessor;
use crate::catalog::Catalog;
use crate::error::{A2uiError, Result};
use crate::model::data_context::DataContext;
use crate::protocol::client_to_server::{ActionData, ActionPayload, ClientMessage, ClientPayload};
use crate::protocol::common_types::Action;
use crate::protocol::rpc::CallAgentFunction;

impl MessageProcessor {
    /// Look up a catalog by its exact negotiated identity, without name fallback.
    pub fn catalog(&self, id: &str) -> Option<&Catalog> {
        self.catalogs.get(id)
    }

    /// Dispatch a component action originating from a real host user gesture.
    ///
    /// Supports current event actions and immediate local/remote function calls.
    /// Nested asynchronous expressions are rejected by the strict evaluator; a
    /// host must not treat a missing async result as a successful null result.
    pub fn activate_component(
        &mut self,
        surface_id: &str,
        component_id: &str,
        base_path: &str,
    ) -> Result<()> {
        let current_action = {
            let surface = self
                .model
                .get_surface(surface_id)
                .ok_or_else(|| A2uiError::SurfaceNotFound(surface_id.into()))?;
            if surface.current_protocol {
                let components = surface.components.borrow();
                let component = components
                    .get(component_id)
                    .ok_or_else(|| A2uiError::ComponentNotFound(component_id.into()))?;
                let catalog = component.catalog_id().unwrap_or(&surface.catalog_id);
                if !self.catalogs.contains_key(catalog) {
                    return Err(A2uiError::CatalogNotFound(catalog.into()));
                }
                Some(component.properties.get("action").cloned())
            } else {
                None
            }
        };
        if let Some(action) = current_action {
            if let Some(action) = action {
                self.start_current_action(surface_id, component_id, base_path, action)?;
            }
            return Ok(());
        }
        let outgoing = {
            let surface = self
                .model
                .get_surface(surface_id)
                .ok_or_else(|| A2uiError::SurfaceNotFound(surface_id.into()))?;
            let components = surface.components.borrow();
            let component = components
                .get(component_id)
                .ok_or_else(|| A2uiError::Validation("action component not found".into()))?;
            let component_catalog = component.catalog_id().unwrap_or(&surface.catalog_id);
            if !self.catalogs.contains_key(component_catalog) {
                return Err(A2uiError::Validation(format!(
                    "unknown component catalog: {component_catalog}"
                )));
            }
            let Some(action) = component.properties.get("action") else {
                return Ok(());
            };
            let action: Action = serde_json::from_value(action.clone())?;
            let data = surface.data_model.borrow();
            match action {
                Action::Event { event } => {
                    let catalog = self
                        .catalogs
                        .get(&surface.catalog_id)
                        .ok_or_else(|| A2uiError::Validation("unknown surface catalog".into()))?;
                    let mut context = scoped_context(&data, &catalog.functions, base_path)
                        .with_catalog_id(&surface.catalog_id)
                        .with_user_activation();
                    if surface.current_protocol {
                        context = context.with_current_protocol();
                    }
                    let values = event
                        .context
                        .iter()
                        .map(|(key, value)| {
                            context
                                .resolve_dynamic_json(&serde_json::to_value(value)?)
                                .map(|value| (key.clone(), value))
                        })
                        .collect::<Result<_>>()?;
                    let user_message = event
                        .user_message
                        .as_ref()
                        .map(|value| {
                            let resolved =
                                context.resolve_dynamic_json(&serde_json::to_value(value)?)?;
                            resolved.as_str().map(str::to_owned).ok_or_else(|| {
                                A2uiError::Validation(
                                    "action userMessage did not resolve to a string".into(),
                                )
                            })
                        })
                        .transpose()?;
                    Some(ClientMessage {
                        version: if surface.current_protocol {
                            "1.0"
                        } else {
                            "v1.0"
                        }
                        .into(),
                        payload: ClientPayload::Action(ActionPayload {
                            action: ActionData {
                                name: event.name,
                                surface_id: surface_id.into(),
                                source_component_id: component_id.into(),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                                context: values,
                                user_message,
                                metadata: None,
                                want_response: None,
                                action_id: None,
                            },
                        }),
                    })
                }
                Action::FunctionCall { mut function_call } => {
                    let catalog_id = function_call
                        .catalog_id
                        .clone()
                        .unwrap_or_else(|| surface.catalog_id.clone());
                    let catalog = self.catalogs.get(&catalog_id).ok_or_else(|| {
                        A2uiError::Validation(format!("unknown function catalog: {catalog_id}"))
                    })?;
                    let mut context = scoped_context(&data, &catalog.functions, base_path)
                        .with_catalog_id(&catalog_id)
                        .with_user_activation();
                    if surface.current_protocol {
                        context = context.with_current_protocol();
                    }
                    if function_call.call.starts_with('@')
                        || catalog.get_function(&function_call.call).is_some()
                    {
                        // Schema-only declarations deliberately fail locally until their
                        // full argument schemas and caller boundaries can be enforced.
                        // Never turn a failed local call into a remote call.
                        context.try_execute_function(&function_call)?;
                        None
                    } else {
                        function_call.args = function_call
                            .args
                            .iter()
                            .map(|(key, value)| {
                                context
                                    .resolve_dynamic_json(value)
                                    .map(|value| (key.clone(), value))
                            })
                            .collect::<Result<_>>()?;
                        function_call.catalog_id = Some(catalog_id);
                        Some(ClientMessage {
                            version: "1.0".into(),
                            payload: ClientPayload::CallAgentFunction(
                                crate::protocol::rpc::CallAgentFunctionPayload {
                                    call_agent_function: CallAgentFunction {
                                        surface_id: surface_id.into(),
                                        function_call_id: uuid::Uuid::new_v4().to_string(),
                                        call_function: function_call,
                                    },
                                },
                            ),
                        })
                    }
                }
            }
        };
        if let Some(message) = outgoing {
            self.queue_outgoing(message);
        }
        Ok(())
    }
}

fn scoped_context<'a>(
    data: &'a crate::model::data_model::DataModel,
    functions: &'a std::collections::HashMap<
        String,
        Box<dyn crate::catalog::FunctionImplementation>,
    >,
    base_path: &str,
) -> DataContext<'a> {
    let context = DataContext::new(data, functions);
    if base_path.is_empty() {
        context
    } else {
        context
            .nested(base_path.trim_start_matches('/'))
            .with_template_index(
                base_path
                    .rsplit('/')
                    .next()
                    .and_then(|index| index.parse().ok()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn processor(action: serde_json::Value) -> MessageProcessor {
        let mut processor = MessageProcessor::new(vec![Catalog::new("test")]);
        processor
            .process_message(
                MessageProcessor::parse_message(
                    &json!({
                        "version":"1.0", "createSurface":{"surfaceId":"s", "catalogId":"test",
                        "dataModel":{"rows":[{"name":"one"},{"name":"two"}]},
                        "components":[{"id":"button","component":"Button","action":action}]}
                    })
                    .to_string(),
                )
                .unwrap(),
            )
            .unwrap();
        processor
    }

    #[test]
    fn current_event_resolves_context_and_user_message() {
        let mut p = processor(
            json!({"event":{"name":"pick","userMessage":{"@path":"name"},
            "context":{"name":{"@path":"name"},"index":{"@call":"@index"}}}}),
        );
        p.activate_component("s", "button", "/rows/1").unwrap();
        let output = serde_json::to_value(p.drain_outgoing().remove(0)).unwrap();
        assert_eq!(output["version"], "1.0");
        assert_eq!(output["action"]["context"]["name"], "two");
        assert_eq!(output["action"]["context"]["index"], 1.0);
        assert_eq!(output["action"]["userMessage"], "two");
        assert!(output["action"].get("wantResponse").is_none());
    }

    #[test]
    fn imperative_remote_calls_are_resolved_and_not_deduplicated() {
        let mut p =
            processor(json!({"functionCall":{"@call":"save","args":{"name":{"@path":"name"}}}}));
        p.activate_component("s", "button", "/rows/1").unwrap();
        p.activate_component("s", "button", "/rows/1").unwrap();
        let outputs = p.drain_outgoing();
        let a = serde_json::to_value(&outputs[0]).unwrap();
        let b = serde_json::to_value(&outputs[1]).unwrap();
        assert_eq!(a["callAgentFunction"]["callFunction"]["catalogId"], "test");
        assert_eq!(
            a["callAgentFunction"]["callFunction"]["args"]["name"],
            "two"
        );
        assert_ne!(
            a["callAgentFunction"]["functionCallId"],
            b["callAgentFunction"]["functionCallId"]
        );
    }

    #[test]
    fn unknown_catalog_and_system_calls_do_not_escape() {
        for call in [
            json!({"@call":"save","catalogId":"unknown"}),
            json!({"@call":"@unknown"}),
        ] {
            let mut p = processor(json!({"functionCall":call}));
            p.activate_component("s", "button", "").unwrap();
            assert_eq!(p.drain_action_errors().len(), 1);
            assert!(
                p.drain_outgoing()
                    .iter()
                    .all(|message| !matches!(message.payload, ClientPayload::CallAgentFunction(_)))
            );
        }
    }
    struct GestureFunction;
    impl crate::catalog::FunctionImplementation for GestureFunction {
        fn name(&self) -> &'static str {
            "gesture"
        }
        fn return_type(&self) -> crate::catalog::function_api::ReturnType {
            crate::catalog::function_api::ReturnType::Boolean
        }
        fn requires_user_activation(&self) -> bool {
            true
        }
        fn execute(
            &self,
            _: &std::collections::HashMap<String, serde_json::Value>,
            _: &DataContext,
        ) -> Result<serde_json::Value> {
            Ok(json!(true))
        }
    }

    #[test]
    fn activation_is_explicit_and_not_granted_to_passive_evaluation() {
        let mut p = processor(json!({"functionCall":{"@call":"gesture"}}));
        p.catalogs
            .get_mut("test")
            .unwrap()
            .functions
            .insert("gesture".into(), Box::new(GestureFunction));
        let catalog = p.catalogs.get("test").unwrap();
        let data = crate::model::data_model::DataModel::new();
        let context = DataContext::new(&data, &catalog.functions);
        let call = serde_json::from_value(json!({"@call":"gesture"})).unwrap();
        assert!(context.try_execute_function(&call).is_err());
        p.activate_component("s", "button", "").unwrap();
        assert!(p.drain_outgoing().is_empty());
    }

    #[test]
    fn schema_only_declared_function_routes_after_policy_checks() {
        let mut p = processor(json!({"functionCall":{"@call":"declared"}}));
        p.catalogs.get_mut("test").unwrap().functions.insert(
            "declared".into(),
            Box::new(crate::catalog::SchemaOnlyFunction::new(
                "declared".into(),
                crate::catalog::function_api::ReturnType::Any,
            )),
        );
        p.activate_component("s", "button", "").unwrap();
        assert!(p.drain_action_errors().is_empty());
        assert!(matches!(
            p.drain_outgoing()[0].payload,
            ClientPayload::CallAgentFunction(_)
        ));
    }
}
