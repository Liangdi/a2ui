//! Minimal catalog — registers the `capitalize` function and standard components.

use std::collections::HashMap;

use crate::core::catalog::function_api::{FunctionImplementation, ReturnType};
use crate::core::catalog::Catalog;
use crate::core::error::A2uiError;
use crate::core::model::data_context::DataContext;
use crate::tui::component_impl::{ComponentRegistry, build_registry};
use crate::tui::components::button::ButtonComponent;
use crate::tui::components::column::ColumnComponent;
use crate::tui::components::row::RowComponent;
use crate::tui::components::text::TextComponent;
use crate::tui::components::text_field::TextFieldComponent;

/// The `capitalize` function from the minimal catalog.
///
/// Takes a `"value"` string argument and returns it with the first character
/// uppercased and the rest left as-is.
pub struct CapitalizeFunction;

impl FunctionImplementation for CapitalizeFunction {
    fn name(&self) -> &'static str {
        "capitalize"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, serde_json::Value>,
        _context: &DataContext,
    ) -> Result<serde_json::Value, A2uiError> {
        let value = args
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let capitalized = capitalize_string(value);
        Ok(serde_json::Value::String(capitalized))
    }
}

/// Capitalize the first character of a string, leaving the rest unchanged.
fn capitalize_string(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            upper + chars.as_str()
        }
    }
}

/// Build the minimal catalog with the `capitalize` function and standard components.
pub fn build_minimal_catalog() -> Catalog {
    Catalog::new("https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json")
        .with_function(Box::new(CapitalizeFunction))
        .with_component(Box::new(TextComponent))
        .with_component(Box::new(RowComponent))
        .with_component(Box::new(ColumnComponent))
        .with_component(Box::new(ButtonComponent))
        .with_component(Box::new(TextFieldComponent))
}

/// Build the component registry for the minimal catalog components.
pub fn build_minimal_registry() -> ComponentRegistry {
    build_registry(vec![
        Box::new(TextComponent),
        Box::new(RowComponent),
        Box::new(ColumnComponent),
        Box::new(ButtonComponent),
        Box::new(TextFieldComponent),
    ])
}
