//! Protected current-only ingress with a frozen negotiated catalog registry.
use super::{
    server_to_client::A2uiMessage,
    strict_current::{StrictCurrentValidator, StrictValidationError},
};
use crate::{
    catalog::schema_backed::SchemaBackedCatalog,
    error::{A2uiError, Result},
    message_processor::MessageProcessor,
};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug)]
pub enum IngressError {
    Validation(StrictValidationError),
    Processing(A2uiError),
}
impl std::fmt::Display for IngressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(e) => write!(f, "{}: {}", e.category, e.message),
            Self::Processing(e) => std::fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for IngressError {}
/// `Ok` always means the message was structurally committed. Subscription
/// refresh can report a post-commit diagnostic; that is not a rollback signal.
#[derive(Debug)]
pub struct IngressAccepted {
    pub post_commit_diagnostic: Option<A2uiError>,
}

pub struct ValidatedMessageProcessor {
    processor: MessageProcessor,
    validator: StrictCurrentValidator,
    default_catalog: String,
}
impl ValidatedMessageProcessor {
    /// Create an empty runtime with exact native/schema registrations. Catalogs
    /// cannot be replaced afterward, and a populated unchecked runtime cannot be
    /// wrapped. The explicit default avoids HashMap iteration selecting a catalog.
    pub fn new(catalogs: Vec<SchemaBackedCatalog>, default_catalog: &str) -> Result<Self> {
        let mut names = BTreeSet::new();
        let mut validator = StrictCurrentValidator::new();
        let mut processor = MessageProcessor::new(vec![]);
        for bound in catalogs {
            let definition = bound.definition().clone();
            let name = definition.catalog_id().to_string();
            if !names.insert(name.clone()) {
                return Err(A2uiError::Validation("duplicate negotiated catalog".into()));
            }
            validator.register_catalog(definition, name == default_catalog)?;
            processor.register_native_catalog(bound.into_catalog(), "1.0")?;
        }
        if !names.contains(default_catalog) {
            return Err(A2uiError::Validation(
                "default catalog is not registered".into(),
            ));
        }
        Ok(Self {
            processor,
            validator,
            default_catalog: default_catalog.into(),
        })
    }
    pub fn process_json_message(
        &mut self,
        wire: &str,
    ) -> std::result::Result<IngressAccepted, IngressError> {
        let value: Value =
            serde_json::from_str(wire).map_err(|e| IngressError::Processing(e.into()))?;
        self.process_value(value)
    }
    pub fn process_message(
        &mut self,
        message: A2uiMessage,
    ) -> std::result::Result<IngressAccepted, IngressError> {
        self.process_value(
            serde_json::to_value(message).map_err(|e| IngressError::Processing(e.into()))?,
        )
    }
    fn process_value(
        &mut self,
        mut value: Value,
    ) -> std::result::Result<IngressAccepted, IngressError> {
        if let Some(create) = value
            .get_mut("createSurface")
            .and_then(Value::as_object_mut)
        {
            if !create.contains_key("catalogId") {
                create.insert(
                    "catalogId".into(),
                    Value::String(self.default_catalog.clone()),
                );
            }
        }
        let staged = self
            .validator
            .prepare_message(&value)
            .map_err(IngressError::Validation)?;
        let message = MessageProcessor::parse_message(&value.to_string())
            .map_err(IngressError::Processing)?;
        let diagnostic = self
            .processor
            .process_validated_ingress(message)
            .map_err(IngressError::Processing)?;
        self.validator.commit(staged);
        Ok(IngressAccepted {
            post_commit_diagnostic: diagnostic,
        })
    }
    /// Owned snapshots never expose interior-mutability handles that could bypass
    /// protected ingress and desynchronize the structural validator.
    pub fn component(
        &self,
        surface: &str,
        id: &str,
    ) -> Option<crate::model::component_model::ComponentModel> {
        self.processor
            .model
            .get_surface(surface)?
            .components
            .borrow()
            .get(id)
            .cloned()
    }
    pub fn data(&self, surface: &str, path: &str) -> Option<Value> {
        self.processor
            .model
            .get_surface(surface)?
            .data_model
            .borrow()
            .get(path)
            .cloned()
    }
    pub fn drain_outgoing(&mut self) -> Vec<super::client_to_server::ClientMessage> {
        self.processor.drain_outgoing()
    }
    /// Trusted host user gesture, never an incoming agent message.
    pub fn activate_component(&mut self, surface: &str, component: &str, base: &str) -> Result<()> {
        self.processor.activate_component(surface, component, base)
    }
    pub fn mount_property(
        &mut self,
        surface: &str,
        component: &str,
        property: &str,
        base: &str,
    ) -> Result<String> {
        self.processor
            .mount_property(surface, component, property, base)
    }
    pub fn unmount_property(&mut self, id: &str) {
        self.processor.unmount_property(id)
    }
    pub fn mounted_value(&self, id: &str) -> Option<Value> {
        self.processor.mounted_value(id)
    }
    pub fn set_agent_transport_available(&mut self, available: bool) {
        self.processor.set_agent_transport_available(available)
    }
}

/// Additive checked ingress for an existing renderer-owned processor. It reads
/// the actual structural snapshot on every call, so trusted host mutations cannot
/// leave a stale shadow graph. It does NOT make the processor's other public
/// mutation APIs checked. Use ValidatedMessageProcessor for an encapsulated runtime.
pub struct CheckedIngress {
    validator: StrictCurrentValidator,
    default_catalog: String,
}
impl CheckedIngress {
    /// Bootstrap both objects from exact native/schema bindings and an empty
    /// processor. An adapter must own this pair and keep unchecked ingress private.
    pub fn bootstrap(
        catalogs: Vec<SchemaBackedCatalog>,
        default_catalog: &str,
    ) -> Result<(Self, MessageProcessor)> {
        let mut definitions = Vec::new();
        let mut processor = MessageProcessor::new(vec![]);
        let mut ids = BTreeSet::new();
        for bound in catalogs {
            let definition = bound.definition().clone();
            if !ids.insert(definition.catalog_id().to_string()) {
                return Err(A2uiError::Validation("duplicate negotiated catalog".into()));
            }
            definitions.push(definition);
            processor.register_native_catalog(bound.into_catalog(), "1.0")?;
        }
        let ingress = Self::new(definitions, default_catalog, &processor)?;
        Ok((ingress, processor))
    }

    /// Definitions must be the host's negotiated schemas for this renderer.
    /// Registry/default configuration is frozen for this ingress object's lifetime.
    pub fn new(
        catalogs: Vec<crate::catalog::CatalogDefinition>,
        default_catalog: &str,
        processor: &MessageProcessor,
    ) -> Result<Self> {
        if !processor.model.is_empty() {
            return Err(A2uiError::Validation(
                "checked ingress requires an empty runtime".into(),
            ));
        }
        let mut validator = StrictCurrentValidator::new();
        let mut ids = BTreeSet::new();
        for catalog in catalogs {
            let id = catalog.catalog_id().to_owned();
            if !ids.insert(id.clone()) {
                return Err(A2uiError::Validation("duplicate negotiated catalog".into()));
            }
            validator.register_catalog(catalog, id == default_catalog)?;
        }
        if !ids.contains(default_catalog) {
            return Err(A2uiError::Validation(
                "default catalog is not registered".into(),
            ));
        }
        if ids
            != processor
                .registered_catalog_ids()
                .into_iter()
                .collect::<BTreeSet<_>>()
        {
            return Err(A2uiError::Validation(
                "negotiated and runtime catalog IDs differ".into(),
            ));
        }
        Ok(Self {
            validator,
            default_catalog: default_catalog.into(),
        })
    }
    pub fn process_json_message(
        &mut self,
        processor: &mut MessageProcessor,
        wire: &str,
    ) -> std::result::Result<IngressAccepted, IngressError> {
        let mut value: Value =
            serde_json::from_str(wire).map_err(|e| IngressError::Processing(e.into()))?;
        if let Some(create) = value
            .get_mut("createSurface")
            .and_then(Value::as_object_mut)
        {
            if !create.contains_key("catalogId") {
                create.insert(
                    "catalogId".into(),
                    Value::String(self.default_catalog.clone()),
                );
            }
        }
        self.validator
            .synchronize_runtime(processor)
            .map_err(IngressError::Validation)?;
        let change = self
            .validator
            .prepare_message(&value)
            .map_err(IngressError::Validation)?;
        let message = MessageProcessor::parse_message(&value.to_string())
            .map_err(IngressError::Processing)?;
        let diagnostic = processor
            .process_validated_ingress(message)
            .map_err(IngressError::Processing)?;
        self.validator.commit(change);
        Ok(IngressAccepted {
            post_commit_diagnostic: diagnostic,
        })
    }
}
