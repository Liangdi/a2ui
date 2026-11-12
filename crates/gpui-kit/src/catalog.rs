//! A separately identified, schema-backed catalog for the native widget subset.
use a2ui_base::catalog::SchemaBackedCatalog;
use a2ui_base::error::Result;
use serde_json::json;

/// Adapter-owned identifier, deliberately distinct from the complete basic catalog.
pub const NATIVE_CATALOG_ID: &str = "urn:a2ui:gpui-kit:catalog:native:1";

/// Derive the released basic schemas and real native functions, excluding media
/// playback widgets. The definition can be sent to an agent for negotiation and
/// supplied to the strict core ingress. It is not a promise of complete visual
/// fidelity or assistive-technology support; see the adapter coverage table.
pub fn native_catalog() -> Result<SchemaBackedCatalog> {
    let basic = SchemaBackedCatalog::basic()?;
    let components: Vec<_> = basic
        .definition()
        .components()
        .keys()
        .filter(|name| !matches!(name.as_str(), "AudioPlayer" | "Video"))
        .cloned()
        .collect();
    let definition = basic.definition().derive(&json!({
        "catalogId": NATIVE_CATALOG_ID,
        "components": components,
        "instructions": "Use this catalog identifier for GPUI Kit native widgets. AudioPlayer and Video are not supported. Markdown text is presented as plain text. Single-selection ChoicePicker uses a native select; multiple-selection uses checkboxes. Input and select accessibility descriptions are not currently supported."
    }))?;
    let mut catalog = basic.into_catalog();
    catalog.id = NATIVE_CATALOG_ID.into();
    catalog
        .components
        .retain(|name, _| definition.components().contains_key(name));
    SchemaBackedCatalog::new(catalog, definition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2ui_base::message_processor::MessageProcessor;
    use a2ui_base::model::component_model::ComponentModel;
    use a2ui_base::protocol::strict_current::StrictCurrentValidator;

    #[test]
    fn native_subset_keeps_real_functions_and_rejects_media() {
        let subset = native_catalog().unwrap();
        assert_eq!(subset.catalog().components.len(), 16);
        assert_eq!(subset.definition().catalog_id(), NATIVE_CATALOG_ID);
        assert!(!subset.definition().components().contains_key("Video"));
        assert!(!subset.definition().components().contains_key("AudioPlayer"));
        assert!(subset.catalog().functions.values().all(|f| f.is_native()));
        assert_eq!(
            subset.catalog().functions.len(),
            SchemaBackedCatalog::basic()
                .unwrap()
                .catalog()
                .functions
                .len()
        );
        let mut strict = StrictCurrentValidator::new();
        strict
            .register_catalog(subset.definition().clone(), true)
            .unwrap();
        let valid = json!({"version":"1.0","createSurface":{
            "surfaceId":"s","catalogId":NATIVE_CATALOG_ID,
            "components":[{"id":"root","component":"Text","text":"Native"}]
        }});
        strict.validate_message(&valid).unwrap();
        let mut media = valid.clone();
        media["createSurface"]["surfaceId"] = json!("media");
        media["createSurface"]["components"] =
            json!([{"id":"root","component":"Video","url":"https://example.com/video.mp4"}]);
        assert!(strict.validate_message(&media).is_err());
        let processor = MessageProcessor::new(vec![subset.into_catalog()]);
        let model: ComponentModel =
            ComponentModel::from_json(&json!({"id":"root","component":"Text","text":"Native"}))
                .unwrap();
        assert!(crate::walker::check_native_catalog(&processor, NATIVE_CATALOG_ID, &model).is_ok());
    }
}
