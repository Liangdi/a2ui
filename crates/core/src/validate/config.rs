//! Validation configuration + STRICT/RELAXED presets.
//!
//! Ports `ValidationConfig` from Python `validator.py`. The catalog-schema
//! portions of the Python validator are NOT ported (Rust has no runtime catalog
//! schema); only the three tolerance flags that govern integrity/topology
//! behavior are kept.

/// Tolerance flags for component validation. Defaults (= STRICT) reject
/// orphans, dangling references, and a missing root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidationConfig {
    /// If true, components unreachable from root are allowed (incremental
    /// updates, partial trees).
    pub allow_orphan_components: bool,
    /// If true, references to component IDs not present in this batch are
    /// allowed (the referenced component may already live on the client).
    pub allow_dangling_references: bool,
    /// If true, a missing `root` component is allowed (incremental update
    /// without a createSurface).
    pub allow_missing_root: bool,
}

impl ValidationConfig {
    /// Reject everything: orphans, dangling refs, missing root. The default.
    pub const STRICT: Self = Self {
        allow_orphan_components: false,
        allow_dangling_references: false,
        allow_missing_root: false,
    };

    /// Allow everything: useful for lenient / best-effort loading.
    pub const RELAXED: Self = Self {
        allow_orphan_components: true,
        allow_dangling_references: true,
        allow_missing_root: true,
    };
}

/// Convenience alias matching the Python constant name.
pub const STRICT_VALIDATION: ValidationConfig = ValidationConfig::STRICT;
/// Convenience alias matching the Python constant name.
pub const RELAXED_VALIDATION: ValidationConfig = ValidationConfig::RELAXED;

#[cfg(test)]
#[allow(clippy::assertions_on_constants)] // asserts intentionally spell out the
                                           // known-const config flags as docs
mod tests {
    use super::*;

    #[test]
    fn strict_rejects_everything() {
        assert!(!STRICT_VALIDATION.allow_orphan_components);
        assert!(!STRICT_VALIDATION.allow_dangling_references);
        assert!(!STRICT_VALIDATION.allow_missing_root);
        assert_eq!(STRICT_VALIDATION, ValidationConfig::STRICT);
    }

    #[test]
    fn relaxed_allows_everything() {
        assert!(RELAXED_VALIDATION.allow_orphan_components);
        assert!(RELAXED_VALIDATION.allow_dangling_references);
        assert!(RELAXED_VALIDATION.allow_missing_root);
        assert_eq!(RELAXED_VALIDATION, ValidationConfig::RELAXED);
    }

    #[test]
    fn default_is_strict() {
        assert_eq!(ValidationConfig::default(), ValidationConfig::STRICT);
    }
}
