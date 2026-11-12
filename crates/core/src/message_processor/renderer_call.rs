//! Shared policy boundary for resolved renderer-originated calls.
use super::MessageProcessor;
use crate::catalog::function_api::{AllowedCallers, ReturnType};
use crate::error::{A2uiError, Result};
use crate::model::data_context::DataContext;
use crate::protocol::common_types::FunctionCall;
use serde_json::Value;

/// A checked call is either locally complete or ready to send to the agent.
#[derive(Debug)]
pub enum RendererCall {
    Local(Value),
    Remote(FunctionCall),
}

impl MessageProcessor {
    /// Resolve the catalog and enforce declarations on already-resolved arguments.
    /// `user_activated` must only be true for a real host user gesture.
    pub fn prepare_renderer_call(
        &self,
        surface_id: &str,
        base_path: &str,
        mut call: FunctionCall,
        user_activated: bool,
    ) -> Result<RendererCall> {
        let surface = self
            .model
            .get_surface(surface_id)
            .ok_or_else(|| A2uiError::SurfaceNotFound(surface_id.into()))?;
        let catalog_id = call
            .catalog_id
            .clone()
            .unwrap_or_else(|| surface.catalog_id.clone());
        let catalog = self
            .catalogs
            .get(&catalog_id)
            .ok_or_else(|| A2uiError::CatalogNotFound(catalog_id.clone()))?;
        if self
            .catalog_versions
            .get(&catalog_id)
            .is_some_and(|version| version != "1.0" && surface.current_protocol)
        {
            return Err(A2uiError::CatalogNotFound(catalog_id));
        }
        let data = surface.data_model.borrow();
        let mut context = DataContext::new(&data, &catalog.functions)
            .with_catalog_id(&catalog_id)
            .with_catalogs(&self.catalogs, Some(&surface.catalog_id))
            .with_catalog_versions(&self.catalog_versions);
        if !base_path.is_empty() {
            context = context.nested(base_path.trim_start_matches('/'));
        }
        if surface.current_protocol {
            context = context.with_current_protocol();
        }
        if user_activated {
            context = context.with_user_activation();
        }
        if call.call.starts_with('@') {
            return context.try_execute_function(&call).map(RendererCall::Local);
        }
        call.catalog_id = Some(catalog_id.clone());
        if let Some(function) = catalog.get_function(&call.call) {
            if function.allowed_callers() == AllowedCallers::AgentOnly {
                return Err(A2uiError::InvalidFunctionCall(
                    "agentOnly function cannot be called by renderer".into(),
                ));
            }
            if function.requires_user_activation() && !user_activated {
                return Err(A2uiError::InvalidFunctionCall(
                    "function requires user activation".into(),
                ));
            }
            function.validate_args(&call.args)?;
            if function.is_native() {
                let value = function.execute(&call.args, &context)?;
                let valid = match function.return_type_for_protocol(surface.current_protocol) {
                    ReturnType::Any => true,
                    ReturnType::Void => value.is_null(),
                    ReturnType::String => value.is_string(),
                    ReturnType::Number => value.is_number(),
                    ReturnType::Boolean => value.is_boolean(),
                    ReturnType::Array => value.is_array(),
                    ReturnType::Object => value.is_object(),
                };
                if !valid {
                    return Err(A2uiError::TypeMismatch(
                        "function result does not match declared return type".into(),
                    ));
                }
                return Ok(RendererCall::Local(value));
            }
        }
        Ok(RendererCall::Remote(call))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn processor(policy: &str, activation: bool) -> MessageProcessor {
        let mut p = MessageProcessor::new(vec![]);
        p.register_inline_catalog(json!({"catalogId":"c", "protocolVersion":"1.0", "functions":{"lookup":{
            "returnType":"string", "allowedCallers":policy, "requiresUserActivation":activation,
            "args":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"],"additionalProperties":false}
        }}})).unwrap();
        p.process_message(
            MessageProcessor::parse_message(
                r#"{"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"c"}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        p
    }
    fn call(args: Value) -> FunctionCall {
        serde_json::from_value(json!({"@call":"lookup","args":args})).unwrap()
    }
    #[test]
    fn schema_only_routes_only_after_argument_and_caller_validation() {
        let p = processor("rendererOnly", false);
        assert!(matches!(
            p.prepare_renderer_call("s", "", call(json!({"key":"x"})), false),
            Ok(RendererCall::Remote(_))
        ));
        assert!(
            p.prepare_renderer_call("s", "", call(json!({"key":4})), false)
                .is_err()
        );
        assert!(
            p.prepare_renderer_call("s", "", call(json!({})), false)
                .is_err()
        );
        assert!(
            processor("agentOnly", false)
                .prepare_renderer_call("s", "", call(json!({"key":"x"})), true)
                .is_err()
        );
    }
    #[test]
    fn schema_only_activation_cannot_be_forged_by_passive_binding() {
        let p = processor("rendererOnly", true);
        assert!(
            p.prepare_renderer_call("s", "", call(json!({"key":"x"})), false)
                .is_err()
        );
        assert!(matches!(
            p.prepare_renderer_call("s", "", call(json!({"key":"x"})), true),
            Ok(RendererCall::Remote(_))
        ));
    }
    #[test]
    fn external_schema_references_are_not_fetched() {
        let result = crate::catalog::SchemaOnlyFunction::new("remote".into(), ReturnType::Any)
            .with_policy(
                json!({"$ref":"https://example.invalid/private-schema"}),
                AllowedCallers::RendererOnly,
                false,
            );
        assert!(result.is_err());
    }
}
