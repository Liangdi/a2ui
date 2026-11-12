//! Native validation + AccessKit smoke fixture. No agent transport or external effects.
use a2ui_base::message_processor::MessageProcessor;
use a2ui_gpui_kit::{GpuiApp, customize};
use a2ui_tui::catalogs::basic::build_basic_catalog;
use gpui_kit::component::Root;
use gpui_kit::prelude::*;
use gpui_kit::{TitlebarOptions, WindowBounds, WindowOptions, px, size};
use serde_json::json;
use std::collections::HashMap;

fn main() {
    let message = MessageProcessor::parse_message(&json!({
        "version":"1.0","createSurface":{
            "surfaceId":"smoke","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel":{"name":""},"components":[
                {"id":"root","component":"Column","children":["heading","name","status","decorative","submit"]},
                {"id":"heading","component":"Text","text":"Native validation and accessibility","variant":"h2"},
                {"id":"name","component":"TextField","label":"Name","value":{"@path":"/name"},
                 "@accessibility":{"label":"Profile name"},
                 "@checks":[{"condition":{"@call":"required","args":{"value":{"@path":"/name"}}},"message":"Enter a name"}]},
                {"id":"status","component":"Text","text":{"@call":"formatString","args":{"value":"Hello ${/name}"}},
                 "@accessibility":{"label":"Greeting status","live":"polite"}},
                {"id":"decorative","component":"Text","text":"Decorative text remains visible",
                 "@accessibility":{"hidden":true}},
                {"id":"submit","component":"Button","child":"submitLabel","variant":"primary",
                 "@accessibility":{"label":"Send profile","description":"Submits the example profile"},
                 "@checks":[{"condition":{"@call":"required","args":{"value":{"@path":"/name"}}},"message":"Enter a name to continue"}],
                 "action":{"event":{"name":"submit","context":{"name":{"@path":"/name"}}}}},
                {"id":"submitLabel","component":"Text","text":"Submit"}
            ]
        }
    }).to_string()).expect("static smoke fixture is valid");
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        customize(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(960.), px(640.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("A2UI Native Smoke".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_decorations: Some(gpui_kit::WindowDecorations::Client),
            ..Default::default()
        };
        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                let view = cx.new(|_| {
                    let mut app = GpuiApp::new(vec![build_basic_catalog()], HashMap::new());
                    app.processor_mut().set_agent_transport_available(false);
                    app.set_samples(vec![("Native smoke".into(), vec![message])], 0);
                    app
                });
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
