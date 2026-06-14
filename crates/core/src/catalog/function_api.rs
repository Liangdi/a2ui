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

/// A function implementation that can be executed by the A2UI runtime.
pub trait FunctionImplementation: Send + Sync + 'static {
    /// The function name as it appears in the catalog.
    fn name(&self) -> &'static str;

    /// The return type of this function.
    fn return_type(&self) -> ReturnType;

    /// Execute the function with resolved arguments.
    ///
    /// Args are already resolved (dynamic values evaluated) by the DataContext.
    fn execute(
        &self,
        args: &HashMap<String, serde_json::Value>,
        context: &DataContext,
    ) -> Result<serde_json::Value, A2uiError>;
}
