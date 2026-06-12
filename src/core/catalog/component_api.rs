//! Framework-agnostic component API definition.

/// The framework-agnostic definition of a component: its name and schema.
pub trait ComponentApi: Send + Sync + 'static {
    /// The component name as it appears in A2UI JSON (e.g. "Button", "Text").
    fn name(&self) -> &'static str;
}
