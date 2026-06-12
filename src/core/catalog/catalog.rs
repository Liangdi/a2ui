//! Catalog — groups component definitions and function implementations.

use std::collections::HashMap;

use super::component_api::ComponentApi;
use super::function_api::FunctionImplementation;

/// A catalog groups component APIs and function implementations.
pub struct Catalog {
    /// Unique catalog URI identifier.
    pub id: String,
    /// Component implementations keyed by component name.
    pub components: HashMap<String, Box<dyn ComponentApi>>,
    /// Function implementations keyed by function name.
    pub functions: HashMap<String, Box<dyn FunctionImplementation>>,
}

impl Catalog {
    /// Create a new empty catalog.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            components: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    /// Add a component implementation.
    pub fn with_component(mut self, component: Box<dyn ComponentApi>) -> Self {
        self.components.insert(component.name().to_string(), component);
        self
    }

    /// Add a function implementation.
    pub fn with_function(mut self, function: Box<dyn FunctionImplementation>) -> Self {
        self.functions.insert(function.name().to_string(), function);
        self
    }

    /// Look up a component by name.
    pub fn get_component(&self, name: &str) -> Option<&dyn ComponentApi> {
        self.components.get(name).map(|b| b.as_ref())
    }

    /// Look up a function by name.
    pub fn get_function(&self, name: &str) -> Option<&dyn FunctionImplementation> {
        self.functions.get(name).map(|b| b.as_ref())
    }
}
