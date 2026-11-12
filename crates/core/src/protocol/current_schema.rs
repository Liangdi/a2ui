//! Optional full JSON Schema validation against a negotiated, locally supplied catalog.
//! All referenced resources are local; network/file fetching is disabled.
use crate::error::{A2uiError, Result};
use serde_json::Value;

pub struct CurrentSchemaValidator {
    incoming: jsonschema::Validator,
    outgoing: jsonschema::Validator,
}

impl CurrentSchemaValidator {
    /// Build released-1.0 wire validators for a self-contained catalog document.
    /// This validates schemas, not message ordering, topology, or native UI behavior.
    pub fn new(catalog: Value) -> Result<Self> {
        let incoming: Value = serde_json::from_str(include_str!("schemas/agent_to_renderer.json"))?;
        let outgoing: Value = serde_json::from_str(include_str!("schemas/renderer_to_agent.json"))?;
        let common: Value = serde_json::from_str(include_str!("schemas/common_types.json"))?;
        let catalog_id = catalog
            .get("$id")
            .and_then(Value::as_str)
            .unwrap_or("https://a2ui.org/specification/v1_0/catalog.json")
            .to_string();
        let base = catalog_id
            .rsplit_once('/')
            .map(|(base, _)| base)
            .unwrap_or("https://a2ui.org/specification/v1_0");
        let registry = jsonschema::Registry::new()
            .add(
                "https://a2ui.org/specification/v1_0/common_types.json",
                common.clone(),
            )
            .map_err(schema_error)?
            .add(format!("{base}/common_types.json"), common)
            .map_err(schema_error)?
            .add(
                "https://a2ui.org/specification/v1_0/catalog.json",
                catalog.clone(),
            )
            .map_err(schema_error)?
            .add(catalog_id, catalog)
            .map_err(schema_error)?
            .prepare()
            .map_err(|e| A2uiError::Validation(format!("catalog schema resources: {e}")))?;
        let incoming = jsonschema::options()
            .offline()
            .with_registry(&registry)
            .build(&incoming)
            .map_err(|e| A2uiError::Validation(format!("incoming schema: {e}")))?;
        let outgoing = jsonschema::options()
            .offline()
            .with_registry(&registry)
            .build(&outgoing)
            .map_err(|e| A2uiError::Validation(format!("outgoing schema: {e}")))?;
        Ok(Self { incoming, outgoing })
    }

    /// Validate the original wire value before parsing it into typed messages.
    pub fn parse_incoming(&self, wire: &str) -> Result<super::server_to_client::A2uiMessage> {
        let raw: Value = serde_json::from_str(wire)?;
        self.validate_incoming(&raw)?;
        crate::message_processor::MessageProcessor::parse_message(wire)
    }

    pub fn validate_incoming(&self, message: &Value) -> Result<()> {
        self.incoming
            .validate(message)
            .map_err(|e| A2uiError::Validation(e.to_string()))
    }
    pub fn validate_outgoing(&self, message: &Value) -> Result<()> {
        self.outgoing
            .validate(message)
            .map_err(|e| A2uiError::Validation(e.to_string()))
    }
}

fn schema_error(error: impl std::fmt::Display) -> A2uiError {
    A2uiError::Validation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn validates_real_current_catalog_and_rejects_bad_component_properties() {
        let catalog: Value = serde_json::from_str(include_str!(
            "../../tests/conformance/catalogs/basic_v1.json"
        ))
        .unwrap();
        let validator = CurrentSchemaValidator::new(catalog).unwrap();
        let mut message = json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json","components":[{"id":"root","component":"Text","text":"Hello"}]}});
        validator.validate_incoming(&message).unwrap();
        message["createSurface"]["components"][0]["text"] = json!(42);
        assert!(validator.validate_incoming(&message).is_err());
        assert!(
            validator
                .validate_incoming(&json!({"version":"1.0","updateDataModel":{"surfaceId":"s"}}))
                .is_err()
        );
        assert!(validator.validate_outgoing(&json!({"version":"1.0","rendererFunctionResponse":{"functionCallId":"x","value":null}})).is_ok());
    }
}
