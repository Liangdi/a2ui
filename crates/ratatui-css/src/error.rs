//! Error type for the CSS engine.
//!
//! Hand-rolled (no `thiserror`) to keep the dependency surface minimal for an
//! ecosystem crate.

use std::fmt;

/// All errors produced by `ratatui-css`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssError {
    /// A color value could not be parsed (e.g. `"#zzz"`).
    InvalidColor(String),
    /// A selector could not be parsed (e.g. `"..::"`).
    InvalidSelector(String),
    /// A length/sizing value could not be parsed (e.g. `"width: banana"`).
    InvalidLength(String),
    /// A `var(--name)` referenced a variable that is not defined in any token table.
    UndefinedVariable(String),
    /// A `var()` reference chain is too deep or cyclic.
    CircularVariable(String),
}

impl fmt::Display for CssError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidColor(v) => write!(f, "invalid color: {v}"),
            Self::InvalidSelector(v) => write!(f, "invalid selector: {v}"),
            Self::InvalidLength(v) => write!(f, "invalid length: {v}"),
            Self::UndefinedVariable(v) => write!(f, "undefined variable: {v}"),
            Self::CircularVariable(v) => write!(f, "circular variable reference: {v}"),
        }
    }
}

impl std::error::Error for CssError {}

pub type Result<T> = std::result::Result<T, CssError>;
