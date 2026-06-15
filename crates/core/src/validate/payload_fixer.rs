//! LLM-JSON autofixer — ports `payload_fixer.py`.
//!
//! Parses a raw JSON string from an LLM, applying tolerant fixes:
//! 1. normalize smart (curly) quotes to straight quotes,
//! 2. on parse failure, strip trailing commas and retry,
//! 3. wrap a bare top-level object into a one-element list.
//!
//! Returns `Ok(Vec<Value>)` on success, or an `A2uiError` (`Parse` for serde
//! failures, `Validation` for a non-list/non-object top level).

use regex::Regex;
use std::sync::LazyLock;

static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| {
    // A comma followed by optional whitespace and a closing ] or }.
    Regex::new(r",(\s*[\]}])").expect("TRAILING_COMMA is a compile-time-constant regex")
});

/// Parse and autofix a raw LLM JSON payload into a list of JSON values.
pub fn parse_and_fix(payload: &str) -> Result<Vec<serde_json::Value>, crate::error::A2uiError> {
    let normalized = normalize_smart_quotes(payload);

    match parse_inner(&normalized) {
        Ok(vals) => Ok(vals),
        // Retry once after stripping trailing commas.
        Err(first_err) => {
            let fixed = TRAILING_COMMA.replace_all(&normalized, "$1").into_owned();
            if fixed == normalized {
                // No trailing commas found — nothing more we can do.
                return Err(first_err);
            }
            parse_inner(&fixed).map_err(|_| first_err)
        }
    }
}

/// Parse a (already quote-normalized) payload, wrapping a bare object into a
/// list. Maps serde errors to `A2uiError::Parse`, and a non-list/non-object top
/// level to `A2uiError::Validation`.
fn parse_inner(payload: &str) -> Result<Vec<serde_json::Value>, crate::error::A2uiError> {
    let value: serde_json::Value = serde_json::from_str(payload)?;
    match value {
        serde_json::Value::Array(arr) => Ok(arr),
        other => {
            if other.is_object() {
                Ok(vec![other])
            } else {
                Err(crate::error::A2uiError::Validation(
                    "payload is not a JSON list or object".into(),
                ))
            }
        }
    }
}

/// Replace smart (curly) quotes with straight ASCII quotes.
fn normalize_smart_quotes(s: &str) -> String {
    s.replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clean_json_passes_through() {
        let payload = r#"[{"id":"root","component":"Text"}]"#;
        let vals = parse_and_fix(payload).unwrap();
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["id"], json!("root"));
    }

    #[test]
    fn smart_quotes_normalized() {
        // LLMs sometimes emit curly double quotes around keys/values.
        let payload = "[{\u{201C}id\u{201D}: \u{201C}root\u{201D}}]";
        let vals = parse_and_fix(payload).unwrap();
        assert_eq!(vals[0]["id"], json!("root"));
    }

    #[test]
    fn trailing_comma_removed() {
        let payload = r#"[{"id":"root",},{"id":"c1",}]"#;
        let vals = parse_and_fix(payload).unwrap();
        assert_eq!(vals.len(), 2);
        assert_eq!(vals[0]["id"], json!("root"));
        assert_eq!(vals[1]["id"], json!("c1"));
    }

    #[test]
    fn single_object_wrapped_in_list() {
        let payload = r#"{"id":"root","component":"Text"}"#;
        let vals = parse_and_fix(payload).unwrap();
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0]["id"], json!("root"));
    }

    #[test]
    fn broken_json_errors() {
        let payload = r#"not json at all {{{"#;
        assert!(parse_and_fix(payload).is_err());
    }
}
