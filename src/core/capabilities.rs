//! Capabilities negotiation and inline-catalog parsing.
//!
//! This module implements the A2UI v1.0 capabilities handshake types and a
//! lightweight parser for *inline catalogs* — catalog JSON embedded directly in
//! a client's capabilities payload rather than fetched from a URL.
//!
//! Inline catalog functions are **schema-only**: they describe their argument
//! and return shapes but have no native Rust implementation. They are registered
//! into a [`Catalog`](crate::core::catalog::Catalog) via
//! [`SchemaOnlyFunction`](crate::core::catalog::schema_only::SchemaOnlyFunction)
//! so that the existing `handle_call_function` path can discover and reject
//! execution attempts uniformly. Inline catalog *components* have no native
//! renderer and are drawn at render time by the generic fallback renderer
//! ([`GenericComponent`](crate::tui::components::generic::GenericComponent)).

use serde::{Deserialize, Serialize};

use crate::core::error::{A2uiError, Result};

// ---------------------------------------------------------------------------
// Capability envelopes — mirror the v1.0 spec JSON-Schema shapes exactly.
// ---------------------------------------------------------------------------

/// Server-side capabilities advertised by an A2UI agent/server.
///
/// Mirrors the `a2uiServerCapabilities.v1.0` object from the spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerCapabilities {
    /// The catalog IDs the server can generate/send. Not necessarily resolvable URIs.
    #[serde(default, rename = "supportedCatalogIds")]
    pub supported_catalog_ids: Vec<String>,
    /// Whether the server can accept an `inlineCatalogs` array in the client's
    /// capabilities. Defaults to `false` per the spec.
    #[serde(default, rename = "acceptsInlineCatalogs")]
    pub accepts_inline_catalogs: bool,
}

/// The full server capabilities payload, keyed under a `v1.0` protocol version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerCapabilitiesEnvelope {
    /// The capabilities for protocol version 1.0.
    #[serde(rename = "v1.0")]
    pub v1_0: ServerCapabilities,
}

/// Client-side capabilities sent to the server during the handshake.
///
/// Mirrors the `a2uiClientCapabilities.v1.0` object from the spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClientCapabilities {
    /// The URIs of each component/function catalog the client supports.
    #[serde(default, rename = "supportedCatalogIds")]
    pub supported_catalog_ids: Vec<String>,
    /// Inline catalog definitions. Only meaningful if the server declared
    /// `acceptsInlineCatalogs: true`. Stored as raw JSON values so the parser
    /// can extract schema metadata without losing the original definition.
    #[serde(default, rename = "inlineCatalogs", skip_serializing_if = "Vec::is_empty")]
    pub inline_catalogs: Vec<serde_json::Value>,
}

/// The full client capabilities payload, keyed under a `v1.0` protocol version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClientCapabilitiesEnvelope {
    /// The capabilities for protocol version 1.0.
    #[serde(rename = "v1.0")]
    pub v1_0: ClientCapabilities,
}

// ---------------------------------------------------------------------------
// Parsed inline catalog representation
// ---------------------------------------------------------------------------

/// The schema (argument shape + return type) of one inline-catalog function,
/// extracted for registration as a [`SchemaOnlyFunction`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSchema {
    /// The function name as it appears in the catalog's `functions` map key.
    pub name: String,
    /// The declared return type (one of: string, number, boolean, array,
    /// object, any, void).
    pub return_type: String,
    /// The names of declared arguments (keys of `args.properties`).
    pub arg_names: Vec<String>,
}

/// A parsed inline catalog — enough metadata to register its functions and to
/// know which component names should fall back to the generic renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineCatalog {
    /// The catalog's unique identifier (`catalogId`).
    pub catalog_id: String,
    /// Names of components declared in the catalog (no native renderer).
    pub component_names: Vec<String>,
    /// Functions declared in the catalog (schema-only).
    pub functions: Vec<FunctionSchema>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Validate that `name` matches the UAX#31 approximation
/// `^[A-Za-z_][A-Za-z0-9_]*$`. Returns the name on success.
fn validate_name<'a>(name: &'a str, kind: &str) -> Result<&'a str> {
    let mut chars = name.chars();
    let first = chars.next().ok_or_else(|| {
        A2uiError::Validation(format!("inline catalog {kind} name must not be empty"))
    })?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(A2uiError::Validation(format!(
            "invalid inline catalog {kind} name '{name}': must start with a letter or underscore"
        )));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(A2uiError::Validation(format!(
            "invalid inline catalog {kind} name '{name}': may only contain letters, digits, or underscore"
        )));
    }
    Ok(name)
}

/// Parse an inline catalog from a raw JSON value.
///
/// Extracts:
/// - `catalogId` (required, must be a non-empty string),
/// - the keys of the `components` map (validated component names),
/// - for each entry in the `functions` map: its name, `returnType` (required),
///   and the keys of `args.properties` (argument names).
///
/// Every component and function name is validated against
/// `^[A-Za-z_][A-Za-z0-9_]*$` and duplicate names within their respective maps
/// are rejected.
pub fn parse_inline_catalog(json: &serde_json::Value) -> Result<InlineCatalog> {
    let obj = json
        .as_object()
        .ok_or_else(|| A2uiError::Validation("inline catalog must be a JSON object".into()))?;

    let catalog_id = obj
        .get("catalogId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| A2uiError::Validation("inline catalog missing 'catalogId'".into()))?
        .to_string();
    if catalog_id.is_empty() {
        return Err(A2uiError::Validation(
            "inline catalog 'catalogId' must not be empty".into(),
        ));
    }

    // --- components ---
    let mut component_names = Vec::new();
    if let Some(components) = obj.get("components").and_then(|v| v.as_object()) {
        for key in components.keys() {
            validate_name(key, "component")?;
            component_names.push(key.clone());
        }
    }

    // --- functions ---
    let mut functions = Vec::new();
    if let Some(funcs) = obj.get("functions").and_then(|v| v.as_object()) {
        for (key, fval) in funcs {
            validate_name(key, "function")?;
            let fobj = fval.as_object().ok_or_else(|| {
                A2uiError::Validation(format!(
                    "inline catalog function '{key}' must be an object"
                ))
            })?;
            let return_type = fobj
                .get("returnType")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    A2uiError::Validation(format!(
                        "inline catalog function '{key}' missing 'returnType'"
                    ))
                })?
                .to_string();

            // Argument names live under `args.properties`. Per the spec's
            // FunctionCallValidationSchema, `args` is itself nested under
            // `properties` (alongside `call`), so we look there first and fall
            // back to a top-level `args` for simpler inline definitions.
            let mut arg_names = Vec::new();
            let args_obj = fobj
                .get("properties")
                .and_then(|p| p.get("args"))
                .or_else(|| fobj.get("args"))
                .and_then(|v| v.as_object());
            if let Some(args) = args_obj {
                if let Some(props) = args.get("properties").and_then(|v| v.as_object()) {
                    for arg_key in props.keys() {
                        validate_name(arg_key, "function argument")?;
                        arg_names.push(arg_key.clone());
                    }
                }
            }

            functions.push(FunctionSchema {
                name: key.clone(),
                return_type,
                arg_names,
            });
        }
    }

    Ok(InlineCatalog {
        catalog_id,
        component_names,
        functions,
    })
}

// ---------------------------------------------------------------------------
// Client capabilities builder
// ---------------------------------------------------------------------------

/// Builder for [`ClientCapabilities`].
///
/// Construct with [`ClientCapabilitiesBuilder::from_catalog_ids`] (the IDs the
/// client natively supports), then optionally append validated inline catalogs
/// with [`.with_inline_catalog`](Self::with_inline_catalog).
#[derive(Debug, Clone, Default)]
pub struct ClientCapabilitiesBuilder {
    supported_catalog_ids: Vec<String>,
    inline_catalogs: Vec<serde_json::Value>,
}

impl ClientCapabilitiesBuilder {
    /// Start a builder whose `supportedCatalogIds` is the given list.
    pub fn from_catalog_ids(ids: Vec<String>) -> Self {
        Self {
            supported_catalog_ids: ids,
            inline_catalogs: Vec::new(),
        }
    }

    /// Validate and append an inline catalog JSON definition.
    ///
    /// The catalog is parsed (and thus validated) eagerly so malformed inline
    /// catalogs are rejected at build time, not at render time.
    pub fn with_inline_catalog(mut self, json: serde_json::Value) -> Result<Self> {
        // Validate by parsing; discard the result (we store the raw JSON).
        parse_inline_catalog(&json)?;
        self.inline_catalogs.push(json);
        Ok(self)
    }

    /// Finalize into a [`ClientCapabilities`].
    pub fn build(self) -> ClientCapabilities {
        ClientCapabilities {
            supported_catalog_ids: self.supported_catalog_ids,
            inline_catalogs: self.inline_catalogs,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Path to the minimal catalog fixture (relative to the repo root).
    const MINIMAL_CATALOG_PATH: &str =
        "a2ui/specification/v1_0/catalogs/minimal/catalog.json";

    #[test]
    fn parse_minimal_catalog() {
        let content = std::fs::read_to_string(MINIMAL_CATALOG_PATH)
            .expect("minimal catalog fixture should exist");
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let parsed = parse_inline_catalog(&json).expect("should parse minimal catalog");

        assert_eq!(
            parsed.catalog_id,
            "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json"
        );
        // Text, Row, Column, Button, TextField
        assert_eq!(parsed.component_names.len(), 5);
        assert!(parsed.component_names.contains(&"Text".to_string()));
        assert!(parsed.component_names.contains(&"Button".to_string()));

        // capitalize
        assert_eq!(parsed.functions.len(), 1);
        let cap = &parsed.functions[0];
        assert_eq!(cap.name, "capitalize");
        assert_eq!(cap.return_type, "string");
        assert_eq!(cap.arg_names, vec!["value".to_string()]);
    }

    #[test]
    fn reject_bad_name() {
        let bad = json!({
            "catalogId": "test",
            "components": {
                "9BadName": {}
            }
        });
        let err = parse_inline_catalog(&bad).unwrap_err();
        assert!(
            err.to_string().contains("invalid inline catalog component name"),
            "unexpected error: {err}"
        );

        // Also test a function-name violation.
        let bad_fn = json!({
            "catalogId": "test",
            "functions": {
                "has-dash": {"returnType": "string"}
            }
        });
        let err = parse_inline_catalog(&bad_fn).unwrap_err();
        assert!(
            err.to_string().contains("invalid inline catalog function name"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_missing_catalog_id() {
        let bad = json!({"components": {}});
        assert!(parse_inline_catalog(&bad).is_err());
    }

    #[test]
    fn reject_missing_return_type() {
        let bad = json!({
            "catalogId": "test",
            "functions": {
                "noReturn": {}
            }
        });
        let err = parse_inline_catalog(&bad).unwrap_err();
        assert!(err.to_string().contains("missing 'returnType'"));
    }

    #[test]
    fn builder_produces_supported_catalog_ids() {
        let ids = vec![
            "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json".to_string(),
            "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json".to_string(),
        ];
        let caps = ClientCapabilitiesBuilder::from_catalog_ids(ids.clone()).build();
        assert_eq!(caps.supported_catalog_ids, ids);
        assert!(caps.inline_catalogs.is_empty());
    }

    #[test]
    fn builder_appends_inline_catalog() {
        let inline = json!({
            "catalogId": "https://example.com/inline.json",
            "components": {"Greeting": {}},
            "functions": {
                "shout": {
                    "returnType": "string",
                    "args": {
                        "properties": {"value": {}}
                    }
                }
            }
        });
        let caps = ClientCapabilitiesBuilder::from_catalog_ids(vec!["minimal".to_string()])
            .with_inline_catalog(inline.clone())
            .expect("inline catalog should be valid")
            .build();
        assert_eq!(caps.inline_catalogs.len(), 1);
        assert_eq!(caps.inline_catalogs[0], inline);
    }

    #[test]
    fn builder_rejects_invalid_inline_catalog() {
        let bad = json!({"components": {"Bad Name": {}}});
        let res = ClientCapabilitiesBuilder::from_catalog_ids(vec![])
            .with_inline_catalog(bad);
        assert!(res.is_err());
    }

    #[test]
    fn client_capabilities_serializes_camel_case() {
        let caps = ClientCapabilities {
            supported_catalog_ids: vec!["a".to_string()],
            inline_catalogs: vec![],
        };
        let env = ClientCapabilitiesEnvelope { v1_0: caps };
        let json = serde_json::to_value(&env).unwrap();
        assert!(json["v1.0"]["supportedCatalogIds"].is_array());
    }

    #[test]
    fn server_capabilities_serializes_camel_case() {
        let caps = ServerCapabilities {
            supported_catalog_ids: vec!["a".to_string()],
            accepts_inline_catalogs: true,
        };
        let env = ServerCapabilitiesEnvelope { v1_0: caps };
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["v1.0"]["acceptsInlineCatalogs"], true);
        assert!(json["v1.0"]["supportedCatalogIds"].is_array());
    }

    #[test]
    fn server_capabilities_round_trip() {
        let raw = json!({
            "v1.0": {
                "supportedCatalogIds": ["x", "y"],
                "acceptsInlineCatalogs": true
            }
        });
        let env: ServerCapabilitiesEnvelope =
            serde_json::from_value(raw.clone()).expect("should deserialize");
        assert!(env.v1_0.accepts_inline_catalogs);
        assert_eq!(env.v1_0.supported_catalog_ids, vec!["x", "y"]);
    }
}
