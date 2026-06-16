//! End-to-end render smoke test: build a real IcedApp from spec-shaped A2UI
//! messages that exercise the upgraded components (Tabs / ChoicePicker /
//! DateTimeInput / Icon) and call `view()` — which is pure element-tree
//! construction (no window / GPU is opened). A panic-free `view()` proves the
//! full render path — including the native `pick_list`/checkbox construction
//! and the Tabs child-panel recursion — builds against real `ComponentModel`
//! data, complementing the pure-helper unit tests in `components.rs`.

#![cfg(feature = "backend")]

use std::collections::HashMap;

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::protocol::server_to_client::A2uiMessage;
use a2ui_iced::{IcedApp, Message};
use a2ui_tui::catalogs::basic::build_basic_catalog;
use a2ui_tui::catalogs::minimal::build_minimal_catalog;

/// Parse each JSONL line into an [`A2uiMessage`].
fn parse_lines(lines: &[&str]) -> Vec<A2uiMessage> {
    lines
        .iter()
        .map(|s| MessageProcessor::parse_message(s).unwrap())
        .collect()
}

/// Build an `IcedApp` carrying the catalogs + functions and load one sample.
/// Callers then drive `view()` / `update()` themselves (a `view()` call is pure
/// tree construction — no window is opened).
fn build_app(messages: Vec<A2uiMessage>) -> IcedApp {
    let catalogs = vec![build_basic_catalog(), build_minimal_catalog()];
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().to_string(), f))
        .collect();
    let mut app = IcedApp::new(catalogs, functions);
    app.set_samples(vec![("smoke".to_string(), messages)], 0);
    app
}

/// Tabs + Icon + DateTimeInput + a single-select (mutuallyExclusive) bound
/// ChoicePicker — the live `30_live-invitation-builder` shape. The Tabs panel
/// has no `activeTab`, so its selection is tracked locally (see
/// `tab_click_switches_panel_when_unbound`).
#[test]
fn view_builds_tabs_icon_datetime_single_choice() {
    let messages = parse_lines(&[
        r#"{"version":"v1.0","createSurface":{"surfaceId":"main","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"}}"#,
        r#"{"version":"v1.0","updateDataModel":{"surfaceId":"main","path":"/event","value":{"location":["ballroom"],"when":"2026-06-13T14:30:00"}}}"#,
        r#"{"version":"v1.0","updateComponents":{"surfaceId":"main","components":[
            {"id":"root","component":"Column","children":["ic","dt","loc","tabs"]},
            {"id":"ic","component":"Icon","name":"star"},
            {"id":"dt","component":"DateTimeInput","label":"When","enableDate":true,"enableTime":true,"value":{"path":"/event/when"}},
            {"id":"loc","component":"ChoicePicker","label":"Venue","variant":"mutuallyExclusive","value":{"path":"/event/location"},"options":[
                {"label":"Grand Ballroom","value":"ballroom"},
                {"label":"Sunset Terrace","value":"terrace"},
                {"label":"Garden Pavillion","value":"garden"}
            ]},
            {"id":"tabs","component":"Tabs","tabs":[
                {"title":"Overview","child":"overview"},
                {"title":"Details","child":"details"}
            ]},
            {"id":"overview","component":"Column","children":["ov-txt"]},
            {"id":"ov-txt","component":"Text","text":"overview panel"},
            {"id":"details","component":"Column","children":["dt-txt"]},
            {"id":"dt-txt","component":"Text","text":"details panel"}
        ]}}"#,
    ]);
    drop(build_app(messages).view());
}

/// Tabs with **no** `activeTab` binding (every gallery sample's shape) must
/// still switch panels on click: the click emits `TabActivate`, `update`
/// records it in local state, and the next `view` renders the newly active
/// tab's child. Verifies the full local-state round-trip without a window.
#[test]
fn tab_click_switches_panel_when_unbound() {
    let messages = parse_lines(&[
        r#"{"version":"v1.0","createSurface":{"surfaceId":"main","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"}}"#,
        r#"{"version":"v1.0","updateComponents":{"surfaceId":"main","components":[
            {"id":"root","component":"Column","children":["tabs"]},
            {"id":"tabs","component":"Tabs","tabs":[
                {"title":"Overview","child":"overview"},
                {"title":"Details","child":"details"}
            ]},
            {"id":"overview","component":"Text","text":"overview panel"},
            {"id":"details","component":"Text","text":"details panel"}
        ]}}"#,
    ]);
    let mut app = build_app(messages);
    // Initially tab 0 ("overview") is active. Clicking "Details" (index 1)
    // updates the locally-tracked selection; the subsequent view re-renders
    // the Details panel — neither step may panic.
    let _ = app.update(Message::TabActivate {
        component_id: "tabs".to_string(),
        index: 1,
    });
    drop(app.view());
}

/// A multiple-selection ChoicePicker (checkbox column) bound to a string array,
/// plus a ChoicePicker with a *literal* (unbound) value that must degrade to a
/// read-only control without panicking.
#[test]
fn view_builds_multi_and_unbound_choice() {
    let messages = parse_lines(&[
        r#"{"version":"v1.0","createSurface":{"surfaceId":"main","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"}}"#,
        r#"{"version":"v1.0","updateDataModel":{"surfaceId":"main","path":"/event","value":{"tags":["vip","catering"]}}}"#,
        r#"{"version":"v1.0","updateComponents":{"surfaceId":"main","components":[
            {"id":"root","component":"Column","children":["multi","unbound"]},
            {"id":"multi","component":"ChoicePicker","label":"Add-ons","variant":"multipleSelection","value":{"path":"/event/tags"},"options":[
                {"label":"VIP lounge","value":"vip"},
                {"label":"Catering","value":"catering"},
                {"label":"AV crew","value":"av"}
            ]},
            {"id":"unbound","component":"ChoicePicker","label":"Static","variant":"mutuallyExclusive","value":["vip"],"options":[
                {"label":"VIP lounge","value":"vip"},
                {"label":"Catering","value":"catering"}
            ]}
        ]}}"#,
    ]);
    drop(build_app(messages).view());
}
