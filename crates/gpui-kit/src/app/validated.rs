//! Protected released-1.0 embedding: one runtime, no raw model/processor escape.
use super::GpuiApp;
use a2ui_base::catalog::SchemaBackedCatalog;
use a2ui_base::protocol::client_to_server::ClientMessage;
use a2ui_base::protocol::validated_processor::{CheckedIngress, IngressAccepted, IngressError};
use gpui_kit::{App, AppContext, Context, Entity, IntoElement, Render, Window};
use serde_json::Value;
use std::collections::HashMap;

/// A validated embedding view. Unlike the compatibility `GpuiApp`, it exposes no
/// mutable processor, model, sample loader or inner entity. All wire mutations
/// enter the same runtime through frozen schema-backed catalogs and checked
/// ingress. Native widget edits still use that runtime directly.
pub struct ValidatedGpuiApp {
    ingress: CheckedIngress,
    inner: Entity<GpuiApp>,
}

impl ValidatedGpuiApp {
    pub fn new(
        catalogs: Vec<SchemaBackedCatalog>,
        default_catalog: &str,
        cx: &mut App,
    ) -> a2ui_base::error::Result<Self> {
        let (ingress, processor) = CheckedIngress::bootstrap(catalogs, default_catalog)?;
        let inner = cx.new(|_| {
            let mut app = GpuiApp::new(Vec::new(), HashMap::new());
            app.processor = processor;
            app
        });
        Ok(Self { ingress, inner })
    }

    /// Parse, validate and apply exactly once. An accepted result can contain a
    /// post-commit diagnostic: do not retry that message as though it was rejected.
    pub fn process_json_message(
        &mut self,
        wire: &str,
        cx: &mut App,
    ) -> Result<IngressAccepted, IngressError> {
        let ingress = &mut self.ingress;
        self.inner.update(cx, |app, cx| {
            let accepted = apply_checked(ingress, app, wire)?;
            cx.notify();
            Ok(accepted)
        })
    }

    pub fn drain_outgoing_messages(&mut self, cx: &mut App) -> Vec<ClientMessage> {
        self.inner
            .update(cx, |app, _| app.drain_outgoing_messages())
    }

    pub fn drain_agent_function_responses(
        &mut self,
        cx: &mut App,
    ) -> Vec<a2ui_base::protocol::rpc::FunctionResponse> {
        self.inner
            .update(cx, |app, _| app.drain_agent_function_responses())
    }

    /// Controls whether the host handles queued agent requests; does not connect
    /// a network transport or grant permission for external effects.
    pub fn set_agent_transport_available(&mut self, available: bool, cx: &mut App) {
        self.inner.update(cx, |app, _| {
            app.processor.set_agent_transport_available(available);
        });
    }

    /// An owned snapshot only: no RefCell-backed surface/model escapes.
    pub fn data(&self, surface_id: &str, pointer: &str, cx: &App) -> Option<Value> {
        let surface = self
            .inner
            .read(cx)
            .processor
            .model
            .get_surface(surface_id)?;
        surface.data_model.borrow().get(pointer).cloned()
    }
}

impl Render for ValidatedGpuiApp {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.inner.clone()
    }
}

fn apply_checked(
    ingress: &mut CheckedIngress,
    app: &mut GpuiApp,
    wire: &str,
) -> Result<IngressAccepted, IngressError> {
    let accepted = ingress.process_json_message(&mut app.processor, wire)?;
    app.prune_widget_state();
    Ok(accepted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{NATIVE_CATALOG_ID, native_catalog};
    use serde_json::json;

    #[test]
    fn checked_adapter_rejects_invalid_wire_without_replacing_local_edits() {
        let (mut ingress, processor) =
            CheckedIngress::bootstrap(vec![native_catalog().unwrap()], NATIVE_CATALOG_ID).unwrap();
        let mut app = GpuiApp::new(Vec::new(), HashMap::new());
        app.processor = processor;
        let create = json!({"version":"1.0","createSurface":{
            "surfaceId":"s","catalogId":NATIVE_CATALOG_ID,"dataModel":{"name":"before"},
            "components":[{"id":"root","component":"TextField","label":"Name","value":{"@path":"/name"}}]
        }}).to_string();
        apply_checked(&mut ingress, &mut app, &create).unwrap();
        app.set_data_value("/name", json!("native edit"));
        let invalid = json!({"version":"1.0","updateComponents":{
            "surfaceId":"s","components":[{"id":"root","component":"Video","url":"https://example.com/video"}]
        }}).to_string();
        assert!(apply_checked(&mut ingress, &mut app, &invalid).is_err());
        let surface = app.processor.model.get_surface("s").unwrap();
        assert_eq!(
            surface.data_model.borrow().get("/name"),
            Some(&json!("native edit"))
        );
        assert_eq!(
            surface
                .components
                .borrow()
                .get("root")
                .unwrap()
                .component_type,
            "TextField"
        );
        apply_checked(
            &mut ingress,
            &mut app,
            &json!({"version":"1.0","updateDataModel":{
                "surfaceId":"s","path":"/other","value":7
            }})
            .to_string(),
        )
        .unwrap();
        let surface = app.processor.model.get_surface("s").unwrap();
        assert_eq!(
            surface.data_model.borrow().get("/name"),
            Some(&json!("native edit"))
        );
        assert_eq!(surface.data_model.borrow().get("/other"), Some(&json!(7)));
    }
}
