/// Unified error type for all A2UI operations.
#[derive(Debug, thiserror::Error)]
pub enum A2uiError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("surface already exists: {0}")]
    SurfaceExists(String),

    #[error("surface not found: {0}")]
    SurfaceNotFound(String),

    #[error("component not found: {0}")]
    ComponentNotFound(String),

    #[error("invalid function call: {0}")]
    InvalidFunctionCall(String),

    #[error("invalid JSON pointer: {0}")]
    InvalidPointer(String),

    #[error("catalog not found: {0}")]
    CatalogNotFound(String),

    #[error("missing required property '{property}' on component '{component}'")]
    MissingProperty { component: String, property: String },

    #[error("type mismatch: {0}")]
    TypeMismatch(String),

    #[error("no native implementation for function: {0}")]
    NoNativeImplementation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, A2uiError>;
