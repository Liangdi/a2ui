//! Minimal protected native-subset host. No networking or arbitrary code execution.
use a2ui_gpui_kit::{
    ValidatedGpuiApp,
    catalog::{NATIVE_CATALOG_ID, native_catalog},
    customize,
};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, WindowOptions};
use serde_json::json;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        customize(cx);
        cx.open_window(WindowOptions::default(), |window, cx| {
            let mut view = ValidatedGpuiApp::new(vec![native_catalog().expect("bundled catalog")], NATIVE_CATALOG_ID, cx).expect("catalog bootstrap");
            view.set_agent_transport_available(false, cx);
            let result = view.process_json_message(&json!({"version":"1.0","createSurface":{
                "surfaceId":"form","catalogId":NATIVE_CATALOG_ID,"dataModel":{"name":""},
                "components":[{"id":"root","component":"TextField","label":"Your name","value":{"@path":"/name"}}]
            }}).to_string(), cx).expect("static schema-valid fixture");
            // Accepted post-commit diagnostics are not a reason to replay input.
            if let Some(error) = result.post_commit_diagnostic {
                eprintln!("Accepted input; subscription refresh diagnostic: {error}");
            }
            let view = cx.new(|_| view);
            cx.new(|cx| Root::new(view, window, cx))
        }).expect("native window");
        cx.activate(true);
    });
}
