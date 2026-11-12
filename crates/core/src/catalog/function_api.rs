//! Function API and implementation traits for A2UI client-side functions.

use std::collections::HashMap;

use crate::error::A2uiError;
use crate::model::data_context::DataContext;

/// The return type of a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Any,
    Void,
}

/// Which side may initiate a function invocation. The specification defaults
/// functions to renderer-only, so existing implementations remain safe by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AllowedCallers {
    #[default]
    RendererOnly,
    AgentOnly,
    RendererOrAgent,
}

/// A function implementation that can be executed by the A2UI runtime.
pub trait FunctionImplementation: Send + Sync + 'static {
    /// The function name as it appears in the catalog.
    fn name(&self) -> &'static str;

    /// The return type of this function.
    fn return_type(&self) -> ReturnType;

    /// Version-dependent return shape without changing existing implementations.
    fn return_type_for_protocol(&self, _current: bool) -> ReturnType {
        self.return_type()
    }

    /// Remote calls are denied unless explicitly enabled by the implementation.
    fn allowed_callers(&self) -> AllowedCallers {
        AllowedCallers::RendererOnly
    }

    /// Requires a trusted user gesture. Passive evaluation and agent calls are denied.
    /// No activation-capable context is currently exposed by the core.
    fn requires_user_activation(&self) -> bool {
        false
    }

    /// Whether execution is available in this renderer (schema-only declarations return false).
    fn is_native(&self) -> bool {
        true
    }

    /// Validate already-resolved arguments before local execution or remote routing.
    fn validate_args(&self, _args: &HashMap<String, serde_json::Value>) -> Result<(), A2uiError> {
        Ok(())
    }

    /// Execute the function with resolved arguments.
    ///
    /// Args are already resolved (dynamic values evaluated) by the DataContext.
    fn execute(
        &self,
        args: &HashMap<String, serde_json::Value>,
        context: &DataContext,
    ) -> Result<serde_json::Value, A2uiError>;
}
