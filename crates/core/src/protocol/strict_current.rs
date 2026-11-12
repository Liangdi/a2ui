//! Opt-in stateful released-1.0 validation. Independent of renderer mutation.
//! Each message commits its validation state only after all checks succeed.
use super::current_schema::CurrentSchemaValidator;
use crate::catalog::definition::CatalogDefinition;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StrictValidationError {
    pub category: String,
    pub message: String,
    pub surface_id: Option<String>,
    pub path: String,
}
impl StrictValidationError {
    fn new(message: impl Into<String>, surface: Option<&str>, path: &str) -> Self {
        Self {
            category: "ValidationError".into(),
            message: message.into(),
            surface_id: surface.map(str::to_owned),
            path: path.into(),
        }
    }
    pub fn renderer_error(&self) -> Value {
        let mut error =
            json!({"code":"VALIDATION_FAILED", "message":self.message, "path":self.path});
        if let Some(id) = &self.surface_id {
            error["surfaceId"] = json!(id);
        }
        json!({"version":"1.0", "error":error})
    }
}
#[derive(Clone, Default)]
struct Surface {
    catalog: String,
    components: BTreeMap<String, Value>,
}
/// Catalogs are host supplied, never fetched. No renderer or transport side effects.
/// Register all used catalogs before processing; unknown catalogs fail closed.
/// This validates structural integrity, not RPC authorization or lifecycle.
/// Catalogs should not be replaced after surfaces have been validated.
/// Limits: 1,024 surfaces, 10,000 components per surface/batch, 50 nested levels,
/// 5 nested calls, 64 reference-schema levels and 100,000 traversal visits.
#[derive(Default)]
pub struct StrictCurrentValidator {
    catalogs: BTreeMap<String, CatalogDefinition>,
    surfaces: BTreeMap<String, Surface>,
    default_catalog: Option<String>,
}
impl StrictCurrentValidator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register_catalog(&mut self, catalog: CatalogDefinition, default: bool) {
        let id = catalog.catalog_id().to_string();
        if default {
            self.default_catalog = Some(id.clone());
        }
        self.catalogs.insert(id, catalog);
    }
    pub fn validate_message(&mut self, message: &Value) -> Result<(), StrictValidationError> {
        let err = |s: &str| StrictValidationError::new(s, None, "");
        if message["version"] != "1.0" {
            return Err(err("Expected released version 1.0"));
        }
        let kind = [
            "createSurface",
            "updateComponents",
            "updateDataModel",
            "deleteSurface",
            "callRendererFunction",
            "agentFunctionResponse",
        ]
        .into_iter()
        .filter(|k| message.get(k).is_some())
        .collect::<Vec<_>>();
        if kind.len() != 1 {
            return Err(err("Expected exactly one message payload"));
        }
        let kind = kind[0];
        let payload = &message[kind];
        let surface_id = payload["surfaceId"].as_str();
        let mut candidate = surface_id
            .and_then(|s| self.surfaces.get(s))
            .cloned()
            .unwrap_or_default();
        if kind == "createSurface"
            && surface_id.is_some_and(|id| !self.surfaces.contains_key(id))
            && self.surfaces.len() >= 1_024
        {
            return Err(err("Surface limit exceeded"));
        }
        if kind == "createSurface" {
            candidate = Surface::default();
            candidate.catalog = payload["catalogId"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| self.default_catalog.clone())
                .ok_or_else(|| err("No default catalog registered"))?;
        } else if candidate.catalog.is_empty() {
            candidate.catalog = self.default_catalog.clone().unwrap_or_default();
        }
        let catalog = self
            .catalogs
            .get(&candidate.catalog)
            .ok_or_else(|| err("Unknown catalog"))?;
        if let Some(components) = payload.get("components").and_then(Value::as_array) {
            if components.len() > 10_000 {
                return Err(err("Component batch limit exceeded"));
            }
            for component in components {
                check_depth(component, 0, 0, surface_id, "/components", true)?;
            }
        }
        if kind == "updateDataModel" {
            check_depth(&payload["value"], 0, 0, surface_id, "/value", false)?;
        }
        CurrentSchemaValidator::new(wire_catalog(catalog))
            .and_then(|v| v.validate_incoming(message))
            .map_err(|e| StrictValidationError::new(e.to_string(), surface_id, ""))?;
        // Directive paths only: ordinary literal dictionaries named `path` are data.
        if kind == "updateDataModel" {
            if let Some(path) = payload["path"].as_str() {
                if !path.is_empty() && !path.starts_with('/') {
                    return Err(StrictValidationError::new(
                        "Invalid path syntax: updateDataModel requires an absolute JSON Pointer",
                        surface_id,
                        "/path",
                    ));
                }
                check_path(path, surface_id, "/path")?;
            }
            check_depth(&payload["value"], 0, 0, surface_id, "/value", false)?;
        }
        if let Some(components) = payload.get("components").and_then(Value::as_array) {
            let mut ids = BTreeSet::new();
            for (index, component) in components.iter().enumerate() {
                let path = format!("/components/{index}");
                let id = component["id"].as_str().unwrap_or("");
                if !identifier(id) {
                    return Err(StrictValidationError::new(
                        format!("Component id '{id}' must be a valid Unicode identifier"),
                        surface_id,
                        &format!("{path}/id"),
                    ));
                }
                if !ids.insert(id) {
                    return Err(StrictValidationError::new(
                        format!("Duplicate component ID '{id}'"),
                        surface_id,
                        &path,
                    ));
                }
                let component_catalog = component["@catalogId"]
                    .as_str()
                    .unwrap_or(&candidate.catalog);
                let definition = self.catalogs.get(component_catalog).ok_or_else(|| {
                    StrictValidationError::new("Unknown component catalog", surface_id, &path)
                })?;
                definition
                    .validate_component(component)
                    .map_err(|e| StrictValidationError::new(e.to_string(), surface_id, &path))?;
                check_depth(component, 0, 0, surface_id, &path, true)?;
                candidate.components.insert(id.into(), component.clone());
            }
            if candidate.components.len() > 10_000 {
                return Err(err("Surface component limit exceeded"));
            }
            self.check_topology(&candidate, surface_id)?;
        }
        if let Some(id) = surface_id {
            if kind == "deleteSurface" {
                self.surfaces.remove(id);
            } else if kind == "createSurface" || kind == "updateComponents" {
                self.surfaces.insert(id.into(), candidate);
            }
        }
        Ok(())
    }
    fn check_topology(
        &self,
        surface: &Surface,
        id: Option<&str>,
    ) -> Result<(), StrictValidationError> {
        if !surface.components.contains_key("root") {
            return Err(StrictValidationError::new(
                "Missing root component",
                id,
                "/components",
            ));
        }
        let mut edges = BTreeMap::new();
        for (name, component) in &surface.components {
            let catalog = self
                .catalogs
                .get(component["@catalogId"].as_str().unwrap_or(&surface.catalog))
                .unwrap();
            let schema = &catalog.resolved_schema()["components"]
                [component["component"].as_str().unwrap_or("")];
            let mut refs = BTreeSet::new();
            references(
                schema,
                component,
                catalog.resolved_schema(),
                "",
                0,
                &mut refs,
                &mut 100_000,
            )
            .map_err(|()| {
                StrictValidationError::new(
                    "Reference schema nesting limit exceeded",
                    id,
                    "/components",
                )
            })?;
            for (target, field) in &refs {
                if target == name {
                    return Err(StrictValidationError::new(
                        "Self-reference detected",
                        id,
                        &format!("/components/{name}/{field}"),
                    ));
                }
                if !surface.components.contains_key(target) {
                    return Err(StrictValidationError::new(
                        format!(
                            "Dangling reference: component '{name}' references non-existent component '{target}' in field '{field}'"
                        ),
                        id,
                        &format!("/components/{name}/{field}"),
                    ));
                }
            }
            edges.insert(
                name.clone(),
                refs.into_iter()
                    .map(|(target, _)| target)
                    .collect::<Vec<_>>(),
            );
        }
        let mut visited = BTreeSet::new();
        let mut active = BTreeSet::new();
        visit(
            "root",
            &edges,
            0,
            &mut visited,
            &mut active,
            id,
            &mut 100_000,
        )?;
        if let Some(orphan) = surface.components.keys().find(|k| !visited.contains(*k)) {
            return Err(StrictValidationError::new(
                format!("Component '{orphan}' is not reachable from root"),
                id,
                "/components",
            ));
        }
        Ok(())
    }
}
fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || unicode_ident::is_xid_start(c))
        && chars.all(unicode_ident::is_xid_continue)
}
fn check_path(
    path: &str,
    surface: Option<&str>,
    location: &str,
) -> Result<(), StrictValidationError> {
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return Err(StrictValidationError::new(
                "Invalid path syntax",
                surface,
                location,
            ));
        }
    }
    Ok(())
}
fn check_depth(
    value: &Value,
    depth: usize,
    calls: usize,
    surface: Option<&str>,
    path: &str,
    directives: bool,
) -> Result<(), StrictValidationError> {
    let failure = if depth > 50 {
        Some("Global recursion limit exceeded")
    } else if calls > 5 {
        Some("functionCall depth exceeded")
    } else {
        None
    };
    if let Some(message) = failure {
        let mut e = StrictValidationError::new(message, surface, path);
        e.category = "RecursionError".into();
        return Err(e);
    }
    match value {
        Value::Object(map) => {
            if directives {
                if let Some(p) = map.get("@path").and_then(Value::as_str) {
                    check_path(p, surface, path)?;
                }
            }
            let next = calls + usize::from(directives && map.contains_key("@call"));
            for (key, v) in map {
                check_depth(
                    v,
                    depth + 1,
                    next,
                    surface,
                    &format!("{path}/{key}"),
                    directives,
                )?;
            }
        }
        Value::Array(values) => {
            for (i, v) in values.iter().enumerate() {
                check_depth(
                    v,
                    depth + 1,
                    calls,
                    surface,
                    &format!("{path}/{i}"),
                    directives,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn references(
    schema: &Value,
    value: &Value,
    root: &Value,
    path: &str,
    depth: usize,
    out: &mut BTreeSet<(String, String)>,
    budget: &mut usize,
) -> Result<(), ()> {
    if *budget == 0 {
        return Err(());
    }
    *budget -= 1;
    if depth > 64 {
        return Err(());
    }
    if let Some(reference) = schema["$ref"].as_str() {
        if reference.ends_with("/ComponentId") {
            if let Some(id) = value.as_str() {
                out.insert((id.into(), path.into()));
            }
            return Ok(());
        }
        if let Some(pointer) = reference.strip_prefix('#') {
            if let Some(target) =
                crate::catalog::definition::decode_fragment(pointer).and_then(|p| root.pointer(&p))
            {
                references(target, value, root, path, depth + 1, out, budget)?;
            }
        }
    }
    if let (Some(props), Some(object)) = (schema["properties"].as_object(), value.as_object()) {
        for (key, s) in props {
            if let Some(v) = object.get(key) {
                references(
                    s,
                    v,
                    root,
                    &if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    },
                    depth + 1,
                    out,
                    budget,
                )?;
            }
        }
    }
    if let Some(array) = value.as_array() {
        for (i, v) in array.iter().enumerate() {
            references(
                &schema["items"],
                v,
                root,
                &format!("{path}[{i}]"),
                depth + 1,
                out,
                budget,
            )?;
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema[key].as_array() {
            for branch in branches {
                references(branch, value, root, path, depth + 1, out, budget)?;
            }
        }
    }
    Ok(())
}

fn visit(
    node: &str,
    edges: &BTreeMap<String, Vec<String>>,
    depth: usize,
    visited: &mut BTreeSet<String>,
    active: &mut BTreeSet<String>,
    surface: Option<&str>,
    budget: &mut usize,
) -> Result<(), StrictValidationError> {
    if *budget == 0 {
        return Err(StrictValidationError::new(
            "Topology traversal budget exceeded",
            surface,
            "/components",
        ));
    }
    *budget -= 1;
    if depth > 50 {
        let mut e =
            StrictValidationError::new("Global recursion limit exceeded", surface, "/components");
        e.category = "RecursionError".into();
        return Err(e);
    }
    if !active.insert(node.into()) {
        return Err(StrictValidationError::new(
            "Circular component reference: Circular reference detected",
            surface,
            "/components",
        ));
    }
    visited.insert(node.into());
    for child in &edges[node] {
        visit(child, edges, depth + 1, visited, active, surface, budget)?;
    }
    active.remove(node);
    Ok(())
}

fn wire_catalog(catalog: &CatalogDefinition) -> Value {
    let mut raw = catalog.to_json();
    if !raw["$defs"].is_object() {
        raw["$defs"] = json!({});
    }
    for (name, entries) in [
        ("anyComponent", catalog.components()),
        ("anyFunction", catalog.functions()),
    ] {
        if raw["$defs"].get(name).is_none() {
            let group = if name == "anyComponent" {
                "components"
            } else {
                "functions"
            };
            raw["$defs"][name] = if entries.is_empty() {
                json!(false)
            } else {
                json!({"oneOf":entries.keys().map(|key|json!({"$ref":format!("#/{group}/{}", fragment(key))})).collect::<Vec<_>>()})
            };
        }
    }
    raw
}

fn fragment(key: &str) -> String {
    key.replace('~', "~0")
        .replace('/', "~1")
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
