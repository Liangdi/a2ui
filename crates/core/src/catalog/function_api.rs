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

/// Data mutations declared by a trusted native implementation, never by wire schema.
/// Paths resolve like data bindings against the invoking surface and template base.
/// An absolute pointer selects the surface root. A batch is atomic.
#[derive(Debug, Clone, PartialEq)]
pub enum DataEffect {
    Set {
        path: String,
        value: serde_json::Value,
    },
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

    /// Requires a trusted user gesture. Passive and ordinary wire calls are denied.
    /// Only the host may grant activation through an explicit action/gesture API.
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

    /// Apply declared defaults and validate arguments before routing.
    fn prepare_args(&self, args: &mut HashMap<String, serde_json::Value>) -> Result<(), A2uiError> {
        self.validate_args(args)
    }

    /// Purely describe data writes for a trusted action invocation. Default: none.
    /// The processor validates the whole batch before execute, then commits only
    /// after successful execution and return validation. No external side effects
    /// belong in this method. Passive/synchronous evaluation rejects these commands.
    fn data_effects(
        &self,
        _args: &HashMap<String, serde_json::Value>,
        _context: &DataContext,
    ) -> Result<Vec<DataEffect>, A2uiError> {
        Ok(Vec::new())
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
