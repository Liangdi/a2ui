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
    pub code: String,
    pub message: String,
    pub surface_id: Option<String>,
    pub path: String,
}
impl StrictValidationError {
    fn new(message: impl Into<String>, surface: Option<&str>, path: &str) -> Self {
        Self {
            category: "ValidationError".into(),
            code: "VALIDATION_FAILED".into(),
            message: message.into(),
            surface_id: surface.map(str::to_owned),
            path: path.into(),
        }
    }
    /// Project a correlated wire error without asserting an unverified payload
    /// location. `path` remains the detailed local diagnostic; the wire uses the
    /// root pointer unless the original message is supplied to the method below.
    /// Uncorrelated errors stay local: the official schema requires a surface or
    /// function-call ID even for a generic error. No identifier is fabricated.
    pub fn renderer_error(&self) -> Option<Value> {
        self.project(self.surface_id.as_deref(), None, "")
    }

    /// Recover correlation from one recognized source payload and use a local
    /// path only when it actually resolves in that payload. Graph-ID paths and
    /// missing-field paths fall back to the honest root pointer, not fake indices.
    pub fn renderer_error_for_message(&self, message: &Value) -> Option<Value> {
        let payloads = [
            "createSurface",
            "updateComponents",
            "updateDataModel",
            "deleteSurface",
            "callRendererFunction",
            "agentFunctionResponse",
        ]
        .into_iter()
        .filter_map(|key| message.get(key).map(|payload| (key, payload)))
        .collect::<Vec<_>>();
        if payloads.len() != 1 {
            return self.renderer_error();
        }
        let (kind, payload) = payloads[0];
        let source_surface = if kind == "agentFunctionResponse" {
            None
        } else {
            payload["surfaceId"].as_str()
        };
        let surface = self.surface_id.as_deref().or(source_surface);
        let same_surface = self
            .surface_id
            .as_deref()
            .is_none_or(|id| source_surface == Some(id));
        let path = if same_surface && payload.pointer(&self.path).is_some() {
            self.path.as_str()
        } else {
            ""
        };
        let call = if surface.is_none()
            && matches!(kind, "callRendererFunction" | "agentFunctionResponse")
        {
            payload["functionCallId"].as_str()
        } else {
            None
        };
        self.project(surface, call, path)
    }
    fn project(&self, surface: Option<&str>, call: Option<&str>, path: &str) -> Option<Value> {
        if surface.is_none() && call.is_none() {
            return None;
        }
        let validation_code = matches!(
            self.code.as_str(),
            "VALIDATION_FAILED" | "UNALLOWED_PARENT" | "UNALLOWED_CHILD"
        );
        let code = if validation_code && surface.is_none() {
            "INVALID_MESSAGE"
        } else {
            self.code.as_str()
        };
        let mut error = json!({"code":code,"message":self.message});
        if let Some(id) = surface {
            error["surfaceId"] = json!(id);
            if validation_code {
                error["path"] = json!(path);
            }
        } else if let Some(id) = call {
            error["functionCallId"] = json!(id);
        }
        Some(json!({"version":"1.0","error":error}))
    }
}
#[derive(Clone, Default)]
struct Surface {
    catalog: String,
    components: BTreeMap<String, Value>,
}
pub(crate) struct PreparedChange {
    surface_id: Option<String>,
    surface: Option<Surface>,
    delete: bool,
}
/// Catalogs are host supplied, never fetched. No renderer or transport side effects.
/// Register all used catalogs before processing; unknown catalogs fail closed.
/// This validates structural integrity, not RPC authorization or lifecycle.
/// Catalogs should not be replaced after surfaces have been validated.
/// Limits: 1,024 surfaces, 10,000 components per surface/batch, 50 nested levels,
/// 5 nested calls, 64 reference-schema levels and 100,000 traversal visits.
#[derive(Clone, Default)]
pub struct StrictCurrentValidator {
    catalogs: BTreeMap<String, CatalogDefinition>,
    surfaces: BTreeMap<String, Surface>,
    default_catalog: Option<String>,
}
impl StrictCurrentValidator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register_catalog(
        &mut self,
        catalog: CatalogDefinition,
        default: bool,
    ) -> crate::error::Result<()> {
        if !self.surfaces.is_empty() {
            return Err(crate::error::A2uiError::Validation(
                "catalog registry is frozen while surfaces are active".into(),
            ));
        }
        let id = catalog.catalog_id().to_string();
        if default {
            self.default_catalog = Some(id.clone());
        }
        self.catalogs.insert(id, catalog);
        Ok(())
    }
    pub(crate) fn synchronize_runtime(
        &mut self,
        processor: &crate::message_processor::MessageProcessor,
    ) -> Result<(), StrictValidationError> {
        let mut surfaces = BTreeMap::new();
        let mut total = 0;
        for surface in processor.model.surfaces().filter(|s| s.current_protocol) {
            let components = surface.components.borrow();
            total += components.len();
            if components.len() > 10_000 || total > 100_000 || surfaces.len() >= 1_024 {
                return Err(StrictValidationError::new(
                    "Runtime structural capacity exceeded",
                    Some(&surface.id),
                    "/components",
                ));
            }
            let raw = components
                .all()
                .iter()
                .map(|(id, c)| {
                    let mut value = Value::Object(c.properties.clone());
                    value["id"] = Value::String(c.id.clone());
                    value["component"] = Value::String(c.component_type.clone());
                    (id.clone(), value)
                })
                .collect();
            surfaces.insert(
                surface.id.clone(),
                Surface {
                    catalog: surface.catalog_id.clone(),
                    components: raw,
                },
            );
        }
        self.surfaces = surfaces;
        Ok(())
    }
    pub fn validate_message(&mut self, message: &Value) -> Result<(), StrictValidationError> {
        let change = self.prepare_message(message)?;
        self.commit(change);
        Ok(())
    }
    pub(crate) fn commit(&mut self, change: PreparedChange) {
        if let Some(id) = change.surface_id {
            if change.delete {
                self.surfaces.remove(&id);
            } else if let Some(surface) = change.surface {
                self.surfaces.insert(id, surface);
            }
        }
    }
    pub(crate) fn prepare_message(
        &self,
        message: &Value,
    ) -> Result<PreparedChange, StrictValidationError> {
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
        if !self.catalogs.contains_key(&candidate.catalog) {
            return Err(err("Unknown catalog"));
        }
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
        CurrentSchemaValidator::validate_envelope(message)
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
                    .validate_component_shape(component)
                    .map_err(|e| StrictValidationError::new(e.to_string(), surface_id, &path))?;
                self.validate_calls(component, &candidate.catalog, surface_id, &path)?;
                check_depth(component, 0, 0, surface_id, &path, true)?;
                candidate.components.insert(id.into(), component.clone());
            }
            if candidate.components.len() > 10_000 {
                return Err(err("Surface component limit exceeded"));
            }
            let existing = surface_id
                .and_then(|id| self.surfaces.get(id))
                .map_or(0, |s| s.components.len());
            let total: usize = self.surfaces.values().map(|s| s.components.len()).sum();
            if total - existing + candidate.components.len() > 100_000 {
                return Err(err("Aggregate component limit exceeded"));
            }
            self.check_topology(&candidate, surface_id)?;
        }
        Ok(PreparedChange {
            surface_id: surface_id.map(str::to_owned),
            surface: if kind == "createSurface" || kind == "updateComponents" {
                Some(candidate)
            } else {
                None
            },
            delete: kind == "deleteSurface",
        })
    }

    fn validate_calls(
        &self,
        value: &Value,
        default: &str,
        surface: Option<&str>,
        path: &str,
    ) -> Result<(), StrictValidationError> {
        match value {
            Value::Object(object) => {
                if object.contains_key("@call") {
                    let target = object
                        .get("catalogId")
                        .and_then(Value::as_str)
                        .unwrap_or(default);
                    let catalog = self.catalogs.get(target).ok_or_else(|| {
                        StrictValidationError::new("Unknown function catalog", surface, path)
                    })?;
                    catalog
                        .validate_function_call_shape(value)
                        .map_err(|e| StrictValidationError::new(e.to_string(), surface, path))?;
                    if let Some(function) = value["@call"]
                        .as_str()
                        .and_then(|name| catalog.functions().get(name))
                    {
                        if function["allowedCallers"] == "agentOnly" {
                            return Err(StrictValidationError::new(
                                "agentOnly function cannot be called by a renderer binding or action",
                                surface,
                                path,
                            ));
                        }
                    }
                }
                for (key, child) in object {
                    self.validate_calls(child, default, surface, &format!("{path}/{key}"))?;
                }
            }
            Value::Array(array) => {
                for (index, child) in array.iter().enumerate() {
                    self.validate_calls(child, default, surface, &format!("{path}/{index}"))?;
                }
            }
            _ => {}
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
            let component_catalog = component["@catalogId"].as_str().unwrap_or(&surface.catalog);
            let definition = self.catalogs.get(component_catalog).ok_or_else(|| {
                StrictValidationError::new("Unknown component catalog", id, "/components")
            })?;
            definition
                .validate_component_shape(component)
                .map_err(|e| StrictValidationError::new(e.to_string(), id, "/components"))?;
            check_depth(component, 0, 0, id, "/components", true)?;
            self.validate_calls(component, &surface.catalog, id, "/components")?;

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
        super::composition::validate_composition(
            &surface.components,
            &edges,
            &self.catalogs,
            &surface.catalog,
            "root",
        )
        .map_err(|cause| {
            let mut error = StrictValidationError::new(
                cause.message,
                id,
                &format!("/components/{}", cause.component_id),
            );
            error.category = "ValidationError".into();
            error.code = cause.code.into();
            error
        })?;
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
        e.code = "RECURSION_LIMIT_EXCEEDED".into();
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
        e.code = "RECURSION_LIMIT_EXCEEDED".into();
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
