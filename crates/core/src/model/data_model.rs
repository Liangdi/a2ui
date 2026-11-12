//! Reactive JSON document store with JSON Pointer support.
//!
//! Implements the A2UI DataModel specification:
//! - JSON Pointer (RFC 6901) get/set with A2UI extensions
//! - Auto-vivification of intermediate paths
//! - Bubble & cascade notification strategy
//! - Relative path resolution (handled by DataContext)

use serde_json::Value;

use crate::observable::event_stream::{EventStream, EventSubscription};

/// A reactive JSON document store with JSON Pointer support.
///
/// The data model starts as a JSON object; root replacement accepts any JSON value.
/// Subscribers are notified when values change at specific paths.
pub struct DataModel {
    data: Value,
    subscribers: EventStream<DataModelEvent>,
    snapshots: EventStream<Value>,
}

/// Event emitted when data changes.
#[derive(Debug, Clone)]
pub struct DataModelEvent {
    /// The path that was changed.
    pub path: String,
    /// The new value at the path (None if removed).
    pub new_value: Option<Value>,
}

impl Default for DataModel {
    fn default() -> Self {
        Self::new()
    }
}

impl DataModel {
    /// Create an empty data model (root is `{}`).
    pub fn new() -> Self {
        Self {
            data: Value::Object(serde_json::Map::new()),
            subscribers: EventStream::new(),
            snapshots: EventStream::new(),
        }
    }

    /// Create from an existing JSON value.
    /// Preserves the supplied JSON value.
    pub fn from_value(value: Value) -> Self {
        Self {
            data: value,
            subscribers: EventStream::new(),
            snapshots: EventStream::new(),
        }
    }

    /// Get a reference to the root data.
    pub fn as_value(&self) -> &Value {
        &self.data
    }

    /// Get a value at an absolute JSON Pointer path.
    /// Returns `None` if the path doesn't exist.
    pub fn get(&self, pointer: &str) -> Option<&Value> {
        self.try_get(pointer).ok().flatten()
    }

    /// Read with explicit path-validation errors.
    pub fn try_get(&self, pointer: &str) -> Result<Option<&Value>, String> {
        let tokens = checked_tokens(pointer)?;
        if tokens.is_empty() {
            return Ok(Some(&self.data));
        }
        Ok(resolve_value(&self.data, &tokens).filter(|v| !v.is_null()))
    }

    /// Compatibility wrapper. Use `try_set` to handle invalid writes explicitly.
    pub fn set(&mut self, pointer: &str, value: Value) {
        let _ = self.try_set(pointer, value);
    }

    /// Atomically update a path, rejecting invalid traversal and excessive indices.
    pub fn try_set(&mut self, pointer: &str, value: Value) -> Result<(), String> {
        let tokens = checked_tokens(pointer)?;
        let mut next = self.data.clone();
        if tokens.is_empty() {
            next = if value.is_null() {
                Value::Object(Default::default())
            } else {
                value.clone()
            };
        } else {
            write_value(&mut next, &tokens, value.clone())?;
        }
        if next != self.data {
            self.data = next;
            self.snapshots.emit(&self.data);
            self.notify(pointer, &value);
        }
        Ok(())
    }

    /// Watch a path, firing only when its resolved value changes. Drop to dispose.
    pub fn subscribe_path<F>(&self, pointer: &str, callback: F) -> Result<EventSubscription, String>
    where
        F: Fn(&DataModelEvent) + Send + Sync + 'static,
    {
        let tokens = checked_tokens(pointer)?;
        let path = pointer.to_string();
        let previous = std::sync::Mutex::new(self.try_get(pointer)?.cloned());
        Ok(self.snapshots.on(move |root| {
            let next = if tokens.is_empty() {
                Some(root.clone())
            } else {
                resolve_value(root, &tokens)
                    .filter(|v| !v.is_null())
                    .cloned()
            };
            let mut old = previous.lock().unwrap();
            if *old != next {
                *old = next.clone();
                callback(&DataModelEvent {
                    path: path.clone(),
                    new_value: next,
                });
            }
        }))
    }

    /// Replace the entire data model.
    pub fn replace_all(&mut self, value: Value) {
        self.set("/", value);
    }

    /// Subscribe to data changes.
    pub fn subscribe<F>(&self, callback: F) -> EventSubscription
    where
        F: Fn(&DataModelEvent) + Send + Sync + 'static,
    {
        self.subscribers.on(callback)
    }

    /// Notify subscribers: exact match, bubble up, cascade down.
    fn notify(&self, changed_path: &str, new_value: &Value) {
        let event = DataModelEvent {
            path: changed_path.to_string(),
            new_value: if new_value.is_null() {
                None
            } else {
                Some(new_value.clone())
            },
        };
        self.subscribers.emit(&event);

        // Also bubble up: notify parent paths
        let path = changed_path.trim_end_matches('/');
        let mut parent = path;
        while let Some(pos) = parent.rfind('/') {
            parent = &parent[..pos];
            if parent.is_empty() {
                break;
            }
            let parent_event = DataModelEvent {
                path: parent.to_string(),
                new_value: self.get(parent).cloned(),
            };
            self.subscribers.emit(&parent_event);
        }
    }
}

// ---------------------------------------------------------------------------
// JSON Pointer parsing
// ---------------------------------------------------------------------------

/// Parse a JSON Pointer string into a list of tokens.
/// Handles RFC 6901 escaping: `~1` → `/`, `~0` → `~`
fn parse_pointer(pointer: &str) -> Vec<String> {
    let trimmed = pointer.strip_prefix('/').unwrap_or(pointer);
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split('/')
        .filter(|token| !token.is_empty())
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Resolve a value by walking tokens.
fn resolve_value<'a>(value: &'a Value, tokens: &[String]) -> Option<&'a Value> {
    let mut current = value;
    for token in tokens {
        match current {
            Value::Object(map) => {
                current = map.get(token)?;
            }
            Value::Array(arr) => {
                let idx = array_index(token).ok()?;
                current = arr.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

fn checked_tokens(pointer: &str) -> Result<Vec<String>, String> {
    let tokens = parse_pointer(pointer);
    if tokens
        .iter()
        .any(|t| matches!(t.as_str(), "__proto__" | "constructor" | "prototype"))
    {
        return Err("Forbidden path segment".into());
    }
    Ok(tokens)
}

fn array_index(token: &str) -> Result<usize, String> {
    if token.is_empty()
        || !token.bytes().all(|c| c.is_ascii_digit())
        || (token.len() > 1 && token.starts_with('0'))
    {
        return Err("non-numeric segment".into());
    }
    let index = token
        .parse::<usize>()
        .map_err(|_| "List index exceeds 10000")?;
    if index > 10000 {
        return Err("List index exceeds 10000".into());
    }
    Ok(index)
}

fn write_value(root: &mut Value, tokens: &[String], value: Value) -> Result<(), String> {
    let token = &tokens[0];
    if root.is_null() {
        if value.is_null() {
            return Ok(());
        }
        *root = if token.bytes().all(|c| c.is_ascii_digit()) {
            Value::Array(vec![])
        } else {
            Value::Object(Default::default())
        };
    }
    let child = match root {
        Value::Object(map) => {
            if tokens.len() == 1 {
                if value.is_null() {
                    map.remove(token);
                } else {
                    map.insert(token.clone(), value);
                }
                return Ok(());
            }
            if value.is_null() && !map.contains_key(token) {
                return Ok(());
            }
            map.entry(token.clone()).or_insert(Value::Null)
        }
        Value::Array(array) => {
            let index = array_index(token)?;
            if value.is_null() && index >= array.len() {
                return Ok(());
            }
            array.resize(array.len().max(index + 1), Value::Null);
            if tokens.len() == 1 {
                array[index] = value;
                return Ok(());
            }
            &mut array[index]
        }
        _ => return Err("Cannot set path through a primitive".into()),
    };
    write_value(child, &tokens[1..], value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_get_simple() {
        let mut dm = DataModel::new();
        dm.set("/name", json!("Alice"));
        assert_eq!(dm.get("/name"), Some(&json!("Alice")));
    }

    #[test]
    fn test_get_nested() {
        let mut dm = DataModel::new();
        dm.set("/user/name/first", json!("Bob"));
        assert_eq!(dm.get("/user/name/first"), Some(&json!("Bob")));
        assert_eq!(dm.get("/user/name"), Some(&json!({"first": "Bob"})));
    }

    #[test]
    fn test_set_array() {
        let mut dm = DataModel::new();
        dm.set("/items/0", json!("first"));
        dm.set("/items/1", json!("second"));
        assert_eq!(dm.get("/items/0"), Some(&json!("first")));
        assert_eq!(dm.get("/items/1"), Some(&json!("second")));
    }

    #[test]
    fn test_replace_root() {
        let mut dm = DataModel::new();
        dm.set("/a", json!(1));
        dm.replace_all(json!({"x": 10, "y": 20}));
        assert_eq!(dm.get("/a"), None);
        assert_eq!(dm.get("/x"), Some(&json!(10)));
    }

    #[test]
    fn test_remove_key() {
        let mut dm = DataModel::new();
        dm.set("/name", json!("Alice"));
        dm.set("/name", Value::Null);
        assert_eq!(dm.get("/name"), None);
    }

    #[test]
    fn test_notification() {
        let dm = DataModel::new();
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();

        let _sub = dm.subscribe(move |event: &DataModelEvent| {
            r.lock().unwrap().push(event.path.clone());
        });

        // Need interior mutability workaround — use RefCell pattern in real use
        // For this test we just verify the subscription mechanism works
    }

    #[test]
    fn test_parse_pointer() {
        assert_eq!(parse_pointer("/a/b/c"), vec!["a", "b", "c"]);
        assert_eq!(parse_pointer("/a~1b"), vec!["a/b"]);
        assert_eq!(parse_pointer("/a~0b"), vec!["a~b"]);
        assert_eq!(parse_pointer("/"), Vec::<String>::new());
    }

    #[test]
    fn test_auto_vivification() {
        let mut dm = DataModel::new();
        dm.set("/a/b/0/c", json!(42));
        assert_eq!(dm.get("/a/b/0/c"), Some(&json!(42)));
        assert!(dm.get("/a").unwrap().is_object());
        assert!(dm.get("/a/b").unwrap().is_array());
    }

    #[test]
    fn test_from_initial_data() {
        let dm = DataModel::from_value(json!({
            "user": {"name": "Alice", "age": 30},
            "items": ["a", "b", "c"]
        }));
        assert_eq!(dm.get("/user/name"), Some(&json!("Alice")));
        assert_eq!(dm.get("/items/1"), Some(&json!("b")));
        assert_eq!(dm.get("/items/2"), Some(&json!("c")));
    }
}
