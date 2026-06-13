//! A single A2UI component's configuration.

use serde_json::Value;

/// Represents one component in the flat component map.
#[derive(Debug, Clone)]
pub struct ComponentModel {
    /// Unique component ID within the surface.
    pub id: String,
    /// Component type name (e.g. "Text", "Button", "Column").
    pub component_type: String,
    /// All component properties as raw JSON (type-specific).
    pub properties: serde_json::Map<String, Value>,
}

impl ComponentModel {
    /// Parse from a raw JSON value.
    /// Extracts `id` and `component` fields, puts the rest into `properties`.
    pub fn from_json(value: &Value) -> Result<Self, crate::core::error::A2uiError> {
        let obj = value
            .as_object()
            .ok_or_else(|| crate::core::error::A2uiError::Validation("component must be an object".into()))?;

        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::core::error::A2uiError::Validation("component missing 'id'".into()))?
            .to_string();

        let component_type = obj
            .get("component")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::core::error::A2uiError::Validation(format!("component '{}' missing 'component' type", id)))?
            .to_string();

        // Collect remaining fields as properties (excluding id, component)
        let properties: serde_json::Map<String, Value> = obj
            .iter()
            .filter(|(k, _)| *k != "id" && *k != "component")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Self {
            id,
            component_type,
            properties,
        })
    }

    /// Get a typed property value.
    pub fn get_property<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.properties.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a raw property value.
    pub fn get_raw(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }

    /// Get the `children` property as a ChildList.
    pub fn children(&self) -> Option<crate::core::protocol::common_types::ChildList> {
        self.properties
            .get("children")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get the `child` property as a single ComponentId.
    pub fn child(&self) -> Option<String> {
        self.properties.get("child").and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    /// Get the `action` property.
    pub fn action(&self) -> Option<crate::core::protocol::common_types::Action> {
        self.properties
            .get("action")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get the `weight` property.
    pub fn weight(&self) -> Option<f64> {
        self.properties.get("weight").and_then(|v| v.as_f64())
    }

    /// Get the `checks` property.
    pub fn checks(&self) -> Option<Vec<crate::core::protocol::common_types::CheckRule>> {
        self.properties
            .get("checks")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get the accessibility attributes for this component.
    pub fn accessibility(&self) -> Option<crate::core::protocol::common_types::AccessibilityAttributes> {
        self.properties
            .get("accessibility")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_from_json() {
        let raw = json!({
            "id": "my_button",
            "component": "Button",
            "variant": "primary",
            "child": "button_label"
        });
        let model = ComponentModel::from_json(&raw).unwrap();
        assert_eq!(model.id, "my_button");
        assert_eq!(model.component_type, "Button");
        assert_eq!(model.child(), Some("button_label".to_string()));
    }

    #[test]
    fn test_children_static() {
        let raw = json!({
            "id": "root",
            "component": "Column",
            "children": ["a", "b", "c"]
        });
        let model = ComponentModel::from_json(&raw).unwrap();
        let children = model.children().unwrap();
        match children {
            crate::core::protocol::common_types::ChildList::Static(ids) => {
                assert_eq!(ids, vec!["a", "b", "c"]);
            }
            _ => panic!("expected static child list"),
        }
    }
}
