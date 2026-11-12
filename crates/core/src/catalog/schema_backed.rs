//! An authored schema bound to real native registrations, without changing `Catalog`.
use super::function_api::{AllowedCallers, ReturnType};
use super::{Catalog, CatalogDefinition, ComponentApi};
use crate::error::{A2uiError, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Native registrations and their public metadata. This describes validation and
/// execution, not whether a renderer can visually display every component.
pub struct SchemaBackedCatalog {
    catalog: Catalog,
    definition: CatalogDefinition,
}
impl SchemaBackedCatalog {
    /// Bind an exact native registry to a released definition. Missing/extra names,
    /// schema-only functions, or conflicting return/caller policies are errors.
    pub fn new(catalog: Catalog, definition: CatalogDefinition) -> Result<Self> {
        let raw = definition.to_json();
        if raw["catalogId"] != catalog.id {
            return Err(invalid("catalog ID differs from definition"));
        }
        let component_names: BTreeSet<_> = catalog.components.keys().cloned().collect();
        let function_names: BTreeSet<_> = catalog.functions.keys().cloned().collect();
        if component_names != definition.components().keys().cloned().collect()
            || function_names != definition.functions().keys().cloned().collect()
        {
            return Err(invalid("native registration names differ from definition"));
        }
        for (name, function) in &catalog.functions {
            let schema = &definition.functions()[name];
            let return_type = match schema["returnType"].as_str().unwrap_or("any") {
                "validationResult" => ReturnType::Object,
                other @ ("string" | "number" | "boolean" | "array" | "object" | "void" | "any") => {
                    super::schema_only::parse_return_type(other)
                }
                _ => return Err(invalid("unknown function return type")),
            };
            let callers = match schema["allowedCallers"].as_str().unwrap_or("rendererOnly") {
                "rendererOnly" => AllowedCallers::RendererOnly,
                "agentOnly" => AllowedCallers::AgentOnly,
                "rendererOrAgent" => AllowedCallers::RendererOrAgent,
                _ => return Err(invalid("invalid caller policy")),
            };
            if function.name() != name
                || !function.is_native()
                || function.return_type_for_protocol(true) != return_type
                || function.allowed_callers() != callers
                || function.requires_user_activation()
                    != schema["requiresUserActivation"].as_bool().unwrap_or(false)
            {
                return Err(invalid(&format!("native function policy differs: {name}")));
            }
        }
        for (name, component) in &catalog.components {
            if component.name() != name {
                return Err(invalid("native component key/name mismatch"));
            }
        }
        Ok(Self {
            catalog,
            definition,
        })
    }
    pub fn definition(&self) -> &CatalogDefinition {
        &self.definition
    }
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn to_json(&self) -> Value {
        self.definition.to_json()
    }
    pub fn into_catalog(self) -> Catalog {
        self.catalog
    }

    /// Published basic catalog bound to native functions and framework-independent
    /// schema-validating component definitions. UI registration remains renderer-owned.
    pub fn basic() -> Result<Self> {
        // Production schema source: a2ui-project/a2ui, pinned revision
        // eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41, catalogs/basic/v1/catalog.json.
        let definition = CatalogDefinition::from_json(
            serde_json::from_str(include_str!("basic_catalog.json"))?,
            None,
            None,
        )?;
        let shared = Arc::new(definition.clone());
        let mut catalog = Catalog::new(definition.to_json()["catalogId"].as_str().unwrap());
        for function in super::basic_functions::build_basic_functions() {
            catalog = catalog.with_function(function);
        }
        for name in [
            "Text",
            "Image",
            "Icon",
            "Video",
            "AudioPlayer",
            "Row",
            "Column",
            "List",
            "Card",
            "Tabs",
            "Modal",
            "Divider",
            "Button",
            "TextField",
            "CheckBox",
            "ChoicePicker",
            "Slider",
            "DateTimeInput",
        ] {
            catalog = catalog.with_component(Box::new(DefinedComponent {
                name,
                definition: shared.clone(),
            }));
        }
        Self::new(catalog, definition)
    }
}
struct DefinedComponent {
    name: &'static str,
    definition: Arc<CatalogDefinition>,
}
impl ComponentApi for DefinedComponent {
    fn name(&self) -> &'static str {
        self.name
    }
    fn validate_properties(
        &self,
        properties: &serde_json::Map<String, Value>,
    ) -> std::result::Result<(), Vec<String>> {
        let mut component = Value::Object(properties.clone());
        // ComponentApi receives properties, not an envelope. Keep any explicitly
        // supplied id/name so mismatches are rejected rather than overwritten.
        component
            .as_object_mut()
            .unwrap()
            .entry("id")
            .or_insert(json!("validation"));
        component
            .as_object_mut()
            .unwrap()
            .entry("component")
            .or_insert(json!(self.name));
        if component["component"] != self.name {
            return Err(vec!["component name differs from registration".into()]);
        }
        self.definition
            .validate_component(&component)
            .map_err(|e| vec![e.to_string()])
    }
}
fn invalid(message: &str) -> A2uiError {
    A2uiError::Validation(message.into())
}
