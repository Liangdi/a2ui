//! Validation error types: structured codes + an aggregating report.
//!
//! `A2uiError::Validation(String)` is a single-string leaf and cannot carry the
//! structured (code + path + component_id) multi-error shape that the Python
//! validator collects. `ValidationReport` aggregates `Vec<ValidationError>` and
//! bridges back to the existing error flow via `From<ValidationReport>`.

/// Machine-readable classification of a validation problem.
///
/// Mirrors the distinct failure modes the Python validator distinguishes:
/// integrity (`DuplicateId`, `MissingRoot`, `DanglingReference`), topology
/// (`SelfReference`, `CircularReference`, `OrphanComponent`), and
/// recursion/path (`GlobalDepthExceeded`, `FuncCallDepthExceeded`,
/// `InvalidPathSyntax`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationErrorCode {
    DuplicateId,
    MissingRoot,
    DanglingReference,
    SelfReference,
    CircularReference,
    OrphanComponent,
    GlobalDepthExceeded,
    FuncCallDepthExceeded,
    InvalidPathSyntax,
}

/// A single validation finding.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: ValidationErrorCode,
    pub message: String,
    pub component_id: Option<String>,
    pub path: Option<String>,
}

impl ValidationError {
    // -- convenience constructors (keep call sites clean) --

    pub fn duplicate_id(id: &str) -> Self {
        Self {
            code: ValidationErrorCode::DuplicateId,
            message: format!("Duplicate component ID: {id}"),
            component_id: Some(id.to_string()),
            path: None,
        }
    }

    pub fn missing_root(root_id: &str) -> Self {
        Self {
            code: ValidationErrorCode::MissingRoot,
            message: format!("Missing root component: No component has id='{root_id}'"),
            component_id: Some(root_id.to_string()),
            path: None,
        }
    }

    pub fn dangling(component_id: &str, ref_id: &str, field: &str) -> Self {
        Self {
            code: ValidationErrorCode::DanglingReference,
            message: format!(
                "Component '{component_id}' references non-existent component '{ref_id}' in field '{field}'"
            ),
            component_id: Some(component_id.to_string()),
            path: Some(field.to_string()),
        }
    }

    pub fn self_ref(component_id: &str, field: &str) -> Self {
        Self {
            code: ValidationErrorCode::SelfReference,
            message: format!(
                "Self-reference detected: Component '{component_id}' references itself in field '{field}'"
            ),
            component_id: Some(component_id.to_string()),
            path: Some(field.to_string()),
        }
    }

    pub fn circular(component_id: &str) -> Self {
        Self {
            code: ValidationErrorCode::CircularReference,
            message: format!("Circular reference detected involving component '{component_id}'"),
            component_id: Some(component_id.to_string()),
            path: None,
        }
    }

    pub fn orphan(component_id: &str, root_id: &str) -> Self {
        Self {
            code: ValidationErrorCode::OrphanComponent,
            message: format!("Component '{component_id}' is not reachable from '{root_id}'"),
            component_id: Some(component_id.to_string()),
            path: None,
        }
    }

    pub fn global_depth(component_id: &str) -> Self {
        Self {
            code: ValidationErrorCode::GlobalDepthExceeded,
            message: format!(
                "Global recursion limit exceeded: Depth > {}",
                super::integrity::MAX_GLOBAL_DEPTH
            ),
            component_id: Some(component_id.to_string()),
            path: None,
        }
    }

    pub fn func_depth() -> Self {
        Self {
            code: ValidationErrorCode::FuncCallDepthExceeded,
            message: format!(
                "Recursion limit exceeded: functionCall depth > {}",
                super::integrity::MAX_FUNC_CALL_DEPTH
            ),
            component_id: None,
            path: None,
        }
    }

    pub fn invalid_path(path: &str) -> Self {
        Self {
            code: ValidationErrorCode::InvalidPathSyntax,
            message: format!("Invalid path syntax: '{path}'"),
            component_id: None,
            path: Some(path.to_string()),
        }
    }
}

/// An aggregation of validation findings (may be empty).
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub errors: Vec<ValidationError>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn push(&mut self, e: ValidationError) {
        self.errors.push(e);
    }

    pub fn extend(&mut self, other: ValidationReport) {
        self.errors.extend(other.errors);
    }

    /// Convert into `Ok(())` if there were no errors, otherwise `Err(self)`.
    pub fn into_result(self) -> std::result::Result<(), Self> {
        if self.is_empty() { Ok(()) } else { Err(self) }
    }

    /// Returns the first error matching a code, for test assertions.
    #[cfg(test)]
    pub fn has_code(&self, code: &ValidationErrorCode) -> bool {
        self.errors.iter().any(|e| &e.code == code)
    }
}

impl std::fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msgs: Vec<&str> = self.errors.iter().map(|e| e.message.as_str()).collect();
        write!(f, "{}", msgs.join("\n"))
    }
}

impl From<ValidationReport> for crate::error::A2uiError {
    fn from(report: ValidationReport) -> Self {
        crate::error::A2uiError::Validation(report.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_is_ok() {
        let r = ValidationReport::new();
        assert!(r.is_empty());
        assert!(r.into_result().is_ok());
    }

    #[test]
    fn report_with_errors_is_err() {
        let mut r = ValidationReport::new();
        r.push(ValidationError::duplicate_id("dup"));
        assert!(!r.is_empty());
        assert!(r.into_result().is_err());
    }

    #[test]
    fn display_joins_messages_with_newline() {
        let mut r = ValidationReport::new();
        r.push(ValidationError::duplicate_id("a"));
        r.push(ValidationError::missing_root("root"));
        let s = r.to_string();
        assert!(s.contains("Duplicate component ID: a"));
        assert!(s.contains("Missing root component"));
        assert_eq!(s.matches('\n').count(), 1);
    }

    #[test]
    fn from_report_to_a2ui_error() {
        let mut r = ValidationReport::new();
        r.push(ValidationError::dangling("root", "ghost", "child"));
        let err: crate::error::A2uiError = r.into();
        match err {
            crate::error::A2uiError::Validation(msg) => {
                assert!(msg.contains("references non-existent component 'ghost'"));
            }
            other => panic!("expected Validation variant, got {other:?}"),
        }
    }

    #[test]
    fn extend_merges_reports() {
        let mut a = ValidationReport::new();
        a.push(ValidationError::duplicate_id("x"));
        let mut b = ValidationReport::new();
        b.push(ValidationError::missing_root("root"));
        a.extend(b);
        assert_eq!(a.errors.len(), 2);
    }
}
