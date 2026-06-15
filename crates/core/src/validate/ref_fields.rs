//! The set of property names that hold component references.
//!
//! Rust has no runtime catalog schema (unlike Python, which derives these from
//! JSON Schema via `CatalogSchemaValidator.extract_ref_fields()`). We hardcode
//! the a2ui-standard reference fields here:
//! - `child` (single), `activeTab` (single)
//! - `children` (list: either a `Static` array of ids, or a `Template`
//!   `{componentId, path}` whose `componentId` is a component reference).
//!
//! This matches what `ComponentModel::child()` / `children()` already look at,
//! and what the v0.9-flat samples use.

/// Specification of which property keys carry component references.
#[derive(Debug, Clone)]
pub struct RefFieldSpec {
    /// Keys whose value is a single component id string (e.g. `child`,
    /// `activeTab`).
    pub single_refs: &'static [&'static str],
    /// Keys whose value is a list of component ids — either a JSON array of
    /// strings (`Static`), or a `Template` object `{componentId, path}`
    /// (e.g. `children`).
    pub list_refs: &'static [&'static str],
}

impl RefFieldSpec {
    /// The a2ui-standard reference fields: `child`, `activeTab` (single);
    /// `children` (list).
    pub const DEFAULT: Self = Self {
        single_refs: &["child", "activeTab"],
        list_refs: &["children"],
    };
}

impl Default for RefFieldSpec {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contains_expected_fields() {
        let s = RefFieldSpec::DEFAULT;
        assert!(s.single_refs.contains(&"child"));
        assert!(s.single_refs.contains(&"activeTab"));
        assert!(s.list_refs.contains(&"children"));
    }

    #[test]
    fn default_impl_matches_const() {
        assert_eq!(RefFieldSpec::default().single_refs, RefFieldSpec::DEFAULT.single_refs);
        assert_eq!(RefFieldSpec::default().list_refs, RefFieldSpec::DEFAULT.list_refs);
    }
}
