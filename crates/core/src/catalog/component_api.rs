//! Framework-agnostic component API definition.

/// The framework-agnostic definition of a component: its name and schema.
pub trait ComponentApi: Send + Sync + 'static {
    /// The component name as it appears in A2UI JSON (e.g. "Button", "Text").
    fn name(&self) -> &'static str;

    /// Validate component properties against catalog constraints.
    ///
    /// Returns `Ok(())` if valid, or a `Vec<String>` of validation error messages.
    fn validate_properties(
        &self,
        _properties: &serde_json::Map<String, serde_json::Value>,
    ) -> std::result::Result<(), Vec<String>> {
        Ok(()) // default: no validation
    }
}

// NOTE: Individual TUI components implement ComponentApi manually in the
// tui catalog builders (minimal.rs, basic.rs) via wrapper types or directly.
// A blanket impl from TuiComponent is not possible here because core must
// not depend on the tui layer.
