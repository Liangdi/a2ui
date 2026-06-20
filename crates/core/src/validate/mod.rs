//! A2UI payload & component validation (ported from Python `a2ui_core.validating`).
//!
//! This module provides integrity, topology, recursion/path, and payload-fixing
//! validation that the Python side has but the Rust crate previously lacked.
//! It is v0.9-flat only and does NOT port the Python catalog-schema validator
//! (Rust has no runtime catalog schema).
//!
//! The validators collect ALL problems into a [`ValidationReport`] rather than
//! short-circuiting on the first.

pub mod config;
pub mod error;
pub mod integrity;
pub mod payload_fixer;
pub mod ref_fields;
pub mod topology;

pub use config::{RELAXED_VALIDATION, STRICT_VALIDATION, ValidationConfig};
pub use error::{ValidationError, ValidationErrorCode, ValidationReport};
pub use integrity::{
    MAX_FUNC_CALL_DEPTH, MAX_GLOBAL_DEPTH, get_component_references, validate_component_integrity,
    validate_recursion_and_paths,
};
pub use payload_fixer::parse_and_fix;
pub use ref_fields::RefFieldSpec;
pub use topology::analyze_topology;

/// The conventional root component id (mirrors Python `ROOT_ID`).
pub const ROOT_ID: &str = "root";
