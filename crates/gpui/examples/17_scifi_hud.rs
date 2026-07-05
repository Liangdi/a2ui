//! # Example: Sci-fi HUD — GPUI backend
//!
//! A neon tactical HUD rebuilt on the a2ui protocol, rendered into a real OS
//! window by the GPUI backend. This is the GPUI counterpart of the ratatui-style
//! [`17_scifi_hud`] and the iced `17_scifi_hud`: same data, same "the data model
//! is the only source of truth" architecture, different renderer.
//!
//! Where the ratatui version drew ASCII gauges and a character-grid radar with
//! custom `TuiComponent`s, this one builds GPUI elements directly in `render` —
//! `Progress` gauges, an SVG radar sweep (regenerated each frame from
//! `/radar/angle`), a styled status line, and a rolling event log — and reads
//! every live value from the a2ui data model. No component tree is declared:
//! the layout *is* the `render` method. Only the **data** flows through the
//! protocol.
//!
//! Animation is driven by a GPUI background timer: a [`gpui::Timer`] fires every
//! ~80 ms inside a `cx.spawn` task (the GPUI equivalent of the ratatui loop's
//! `event::poll`), which `tick` turns into the next snapshot. `render` then
//! reads the updated model and rebuilds the element tree.
//!
//! [`17_scifi_hud`]: ../a2ui/examples/17_scifi_hud.rs
//!
//! ## Run
//! ```sh
//! cargo run --manifest-path crates/gpui/Cargo.toml --example 17_scifi_hud --features backend
//! ```
//!
//! Close the window (or the OS's window-close button) to quit.

use std::sync::Arc;
use std::time::Duration;

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::data_model::DataModel;
use a2ui_tui::catalogs::basic::build_basic_catalog;

use gpui::prelude::*;
use gpui::{
    AnyElement, Application, Context, Image, ImageFormat, Render, Timer, TitlebarOptions,
    WindowBounds, WindowOptions, px, relative, size,
};
use gpui_component::progress::Progress;
use gpui_component::TitleBar;
use gpui_component::{Root, h_flex, init as init_component, v_flex};
use gpui_component_assets::Assets;
use serde_json::{Value, json};

// ─── Neon palette (mirrors the ratatui/iced versions' CSS tokens) ────────────

/// Compose an opaque [`gpui::Rgba`] from 8-bit RGB (gpui's own `rgb` is a
/// non-const runtime fn, so a const fn is needed for `const` palette items).
const fn rgb(r: u8, g: u8, b: u8) -> gpui::Rgba {
    gpui::Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

const CYAN: gpui::Rgba = rgb(0x56, 0xF0, 0xFF);
const MAGENTA: gpui::Rgba = rgb(0xFF, 0x4F, 0xD8);
const GREEN: gpui::Rgba = rgb(0x5D, 0xFF, 0xB0);
const AMBER: gpui::Rgba = rgb(0xFF, 0xB4, 0x36);
const BG: gpui::Rgba = rgb(0x04, 0x11, 0x1A);
const PANEL: gpui::Rgba = rgb(0x06, 0x15, 0x1F);
const DIM: gpui::Rgba = rgb(0x2A, 0x6A, 0x7A);
const TEXT: gpui::Rgba = rgb(0xB8, 0xF2, 0xFF);

/// Threshold color for a 0–100 gauge reading: magenta→amber→green as it climbs.
fn value_color(pct: f64) -> gpui::Rgba {
    if pct < 30.0 {
        MAGENTA
    } else if pct < 60.0 {
        AMBER
    } else {
        GREEN
    }
}

/// Color for an event-log severity level.
fn level_color(level: &str) -> gpui::Rgba {
    match level {
        "ok" => GREEN,
        "warn" => AMBER,
        "alert" => MAGENTA,
        _ => DIM,
    }
}

// ─── The HUD view ────────────────────────────────────────────────────────────

/// The HUD's runtime state: the a2ui processor (owns the data model) plus the
/// tick counter and rolling event log — the only "data source" in the app.
struct HudApp {
    processor: MessageProcessor,
    tick: u32,
    /// Newest-first rolling event log.
    events: Vec<(&'static str, &'static str)>,
    /// Cursor into the event `pool` (wraps to simulate a live feed).
    next_pool: usize,
}

impl HudApp {
    /// Build the runtime: a processor seeded with the basic catalog, then a
    /// `createSurface` carrying the *initial* data model.
    fn new(cx: &mut Context<Self>) -> Self {
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
        let create = json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "hud",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "sendDataModel": true,
                "dataModel": {
                    "status": "SYS |  ● ONLINE",
                    "gauges": { "core": 55.0, "pwr": 78.0, "hull": 40.0, "shld": 20.0 },
                    "radar": { "angle": 0.0, "range": 0 },
                    "events": {
                        "fresh": false,
                        "items": [
                            { "msg": "DOCK SEQUENCE OK",   "level": "ok" },
                            { "msg": "RADAR SWEEP DONE",   "level": "" },
                            { "msg": "HULL STRESS +12%",   "level": "warn" },
                            { "msg": "LINK ESTABLISHED",   "level": "ok" },
                            { "msg": "UNKNOWN SIGNATURE",  "level": "alert" },
                            { "msg": "CALIBRATING GYRO",   "level": "" }
                        ]
                    }
                }
            }
        });
        let _ = processor.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap());

        // Animation tick: ~80 ms cadence. Exits when the view is dropped.
        cx.spawn(async move |view, cx| {
            loop {
                Timer::after(Duration::from_millis(80)).await;
                let Ok(_) = view.update(cx, |this, cx| {
                    this.step();
                    cx.notify();
                }) else {
                    break;
                };
            }
        })
        .detach();

        Self {
            processor,
            tick: 0,
            events: EVENT_POOL[..6].to_vec(),
            next_pool: 6,
        }
    }

    /// One clock tick: compute the next telemetry snapshot and ship it as a
    /// single `updateDataModel` message.
    fn step(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let tf = self.tick as f64;

        let spinner = ['|', '/', '—', '\\'][((self.tick / 3) as usize) % 4];
        let status = format!("SYS {spinner}  ● ONLINE");
        let gauges = json!({
            "core": 55.0 + (tf * 0.07).sin() * 18.0,
            "pwr":  78.0 + (tf * 0.05).sin() * 10.0,
            "hull": 40.0 + (tf * 0.03).sin() * 35.0,
            "shld": 20.0 + (tf * 0.09).sin() * 60.0,
        });
        let radar = json!({
            "angle": tf * 0.20,
            "range": (1200.0 + (tf * 0.9).sin() * 600.0) as u32,
        });

        if self.tick % 18 == 0 {
            let entry = EVENT_POOL[self.next_pool % EVENT_POOL.len()];
            self.events.insert(0, entry);
            self.events.truncate(6);
            self.next_pool += 1;
        }
        let fresh = (self.tick % 12) < 4;
        let items: Vec<Value> = self
            .events
            .iter()
            .map(|(msg, level)| json!({ "msg": msg, "level": level }))
            .collect();

        push_snapshot(
            &mut self.processor,
            json!({
                "status": status,
                "gauges": gauges,
                "radar": radar,
                "events": { "fresh": fresh, "items": items },
            }),
        );
    }

    /// Build the HUD: a dark root, then header / body row / footer.
    fn render_hud(&self) -> AnyElement {
        let Some(surface) = self.processor.model.get_surface("hud") else {
            return gpui::div().child("No surface loaded.").into_any_element();
        };
        let model = surface.data_model.borrow();

        let header = self.render_header(&model);
        let telemetry = self.render_telemetry(&model);
        let scanner = self.render_scanner(&model);
        let events = self.render_events(&model);
        let body = h_flex()
            .gap_3()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(panel(telemetry, CYAN).w(relative(0.3)))
            .child(panel(scanner, CYAN).w(relative(0.4)))
            .child(panel(events, DIM).w(relative(0.3)));

        let footer = gpui::div()
            .text_color(DIM)
            .text_size(px(11.))
            .child("[ window-close ] exit   ·   a2ui-driven hud   ·   data flows via updateDataModel");

        v_flex()
            .gap_3()
            .size_full()
            .bg(BG)
            .p_4()
            .child(header)
            .child(body)
            .child(footer)
            .into_any_element()
    }

    // ── Panels ─────────────────────────────────────────────────────────────

    /// Title bar: static title on the left, bound `/status` pinned right.
    fn render_header(&self, model: &DataModel) -> AnyElement {
        let title = gpui::div()
            .text_color(CYAN)
            .text_size(px(18.))
            .child("⟁ A2UI // TACTICAL HUD");
        let status_str = model
            .get("/status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let status = gpui::div().text_color(GREEN).child(status_str);
        h_flex()
            .items_center()
            .gap_2()
            .w_full()
            .child(title)
            .child(gpui::div().flex_1())
            .child(status)
            .into_any_element()
    }

    /// Telemetry: four neon `Progress` gauges bound to `/gauges/*`.
    fn render_telemetry(&self, model: &DataModel) -> AnyElement {
        let defs: [(&str, &str); 4] = [
            ("CORE", "core"),
            ("PWR", "pwr"),
            ("HULL", "hull"),
            ("SHLD", "shld"),
        ];
        let mut col = v_flex().gap_3().child(panel_title("◈ TELEMETRY", CYAN));
        for (label, key) in defs {
            let pct = read_num(model, &format!("/gauges/{key}")).clamp(0.0, 100.0);
            let accent = value_color(pct);
            col = col.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .child(
                        gpui::div()
                            .w(px(46.))
                            .text_color(DIM)
                            .text_size(px(12.))
                            .child(format!("{label:<4}")),
                    )
                    .child(
                        Progress::new()
                            .value(pct as f32)
                            .bg(accent)
                            .h(px(14.))
                            .flex_1(),
                    )
                    .child(
                        gpui::div()
                            .w(px(52.))
                            .text_color(TEXT)
                            .text_size(px(12.))
                            .child(format!("{pct:3.0}%")),
                    ),
            );
        }
        col.into_any_element()
    }

    /// Scanner: an SVG radar sweep (regenerated each frame from `/radar/angle`)
    /// plus a bearing/range readout.
    fn render_scanner(&self, model: &DataModel) -> AnyElement {
        let angle = read_num(model, "/radar/angle");
        let range = read_num(model, "/radar/range") as u32;
        let svg = radar_svg(angle);
        let radar = gpui::img(Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            svg.into_bytes(),
        )))
        .w_full()
        .flex_1()
        .min_h_0();

        let bearing = (angle * 57.2957795) % 360.0;
        let readout = h_flex()
            .items_center()
            .gap_2()
            .child(gpui::div().text_color(DIM).text_size(px(10.)).child("BEARING"))
            .child(gpui::div().text_color(TEXT).text_size(px(12.)).child(format!("{bearing:5.1}°")))
            .child(gpui::div().w(px(16.)))
            .child(gpui::div().text_color(DIM).text_size(px(10.)).child("RANGE"))
            .child(gpui::div().text_color(TEXT).text_size(px(12.)).child(format!("{range:>5}m")));

        v_flex()
            .gap_2()
            .size_full()
            .child(panel_title("◎ SCANNER", CYAN))
            .child(radar)
            .child(readout)
            .into_any_element()
    }

    /// Event log: renders `/events/items`; the newest is highlighted while
    /// `/events/fresh` is true.
    fn render_events(&self, model: &DataModel) -> AnyElement {
        let fresh = model
            .get("/events/fresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut col = v_flex().gap_1().child(panel_title("▤ EVENT LOG", DIM));
        if let Some(arr) = model.get("/events/items").and_then(|v| v.as_array()) {
            for (i, it) in arr.iter().enumerate() {
                let msg = it.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                let level = it.get("level").and_then(|v| v.as_str()).unwrap_or("");
                let c = if i == 0 && fresh { AMBER } else { level_color(level) };
                col = col.child(gpui::div().text_color(c).text_size(px(12.)).child(format!("› {msg}")));
            }
        }
        col.into_any_element()
    }
}

impl Render for HudApp {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Top-level layout mirrors the gpui-component story app's `StoryRoot`
        // (the canonical pattern that renders correctly under GNOME Wayland CSD):
        // an outer `div().size_full()` wrapping a `v_flex` of TitleBar + a
        // `flex_1().overflow_hidden()` content region. (Earlier the content used
        // `min_h_0` with no outer wrapper, which collapsed to a single row under
        // Wayland — see render_hud for the column body layout.)
        gpui::div().size_full().child(
            v_flex()
                .size_full()
                .child(
                    TitleBar::new().child(
                        gpui::div()
                            .text_color(CYAN)
                            .text_size(px(12.))
                            .child("⟁ A2UI // TACTICAL HUD"),
                    ),
                )
                .child(gpui::div().flex_1().overflow_hidden().child(self.render_hud())),
        )
    }
}

// ─── Panel chrome + radar SVG ────────────────────────────────────────────────

/// A small section title in the panel's accent color.
fn panel_title(label: &str, color: gpui::Rgba) -> AnyElement {
    gpui::div()
        .text_color(color)
        .text_size(px(13.))
        .child(label.to_string())
        .into_any_element()
}

/// Wrap content in a dark, neon-bordered panel filling its allotted height.
/// The caller sets the panel's width (e.g. `.w(relative(0.3))`) for the body
/// row's weighted columns.
fn panel(content: AnyElement, border_color: gpui::Rgba) -> gpui::Div {
    gpui::div()
        .bg(PANEL)
        .border_1()
        .border_color(border_color)
        .rounded_md()
        .p_3()
        .h_full()
        .min_w_0()
        .child(content)
}

/// Build the radar as an SVG string: three range rings, a crosshair, a rotating
/// sweep beam + tip pip, and a center pip — all drawn from `angle` (radians).
fn radar_svg(angle: f64) -> String {
    let r = 90.0_f64;
    let (cx, cy) = (100.0, 100.0);
    let tx = cx + angle.cos() * r;
    let ty = cy + angle.sin() * r;
    format!(
        "\
<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 200 200'>\
<rect width='200' height='200' fill='#04111a'/>\
<circle cx='{cx}' cy='{cy}' r='30' fill='none' stroke='#2a6a7a'/>\
<circle cx='{cx}' cy='{cy}' r='60' fill='none' stroke='#2a6a7a'/>\
<circle cx='{cx}' cy='{cy}' r='90' fill='none' stroke='#2a6a7a'/>\
<line x1='10' y1='{cy}' x2='190' y2='{cy}' stroke='#2a6a7a'/>\
<line x1='{cx}' y1='10' x2='{cx}' y2='190' stroke='#2a6a7a'/>\
<line x1='{cx}' y1='{cy}' x2='{tx}' y2='{ty}' stroke='#56f0ff' stroke-width='2'/>\
<circle cx='{tx}' cy='{ty}' r='3' fill='#56f0ff'/>\
<circle cx='{cx}' cy='{cy}' r='3' fill='#ff4fd8'/>\
</svg>"
    )
}

// ─── Driving the HUD ─────────────────────────────────────────────────────────

/// Build + ship one telemetry snapshot as a single `updateDataModel` message.
fn push_snapshot(processor: &mut MessageProcessor, snapshot: Value) {
    let msg = json!({
        "version": "v1.0",
        "updateDataModel": { "surfaceId": "hud", "path": "/", "value": snapshot }
    });
    let _ = processor.process_message(MessageProcessor::parse_message(&msg.to_string()).unwrap());
}

/// Read a numeric data-model binding, defaulting to `0.0`.
fn read_num(model: &DataModel, path: &str) -> f64 {
    model.get(path).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

/// The simulated event feed; entries cycle to mimic a live source.
const EVENT_POOL: &[(&str, &str)] = &[
    ("DOCK SEQUENCE OK", "ok"),
    ("RADAR SWEEP DONE", ""),
    ("HULL STRESS +12%", "warn"),
    ("LINK ESTABLISHED", "ok"),
    ("UNKNOWN SIGNATURE", "alert"),
    ("CALIBRATING GYRO", ""),
    ("POWER SURGE DETECTED", "warn"),
    ("COMMS RELAY UP", "ok"),
    ("DEBRIS FIELD AHEAD", "alert"),
    ("COOLANT NOMINAL", "ok"),
];

fn main() {
    Application::new()
        .with_assets(Assets)
        .run(move |cx| {
            init_component(cx);

            let window_options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1000.), px(680.)), cx)),
                titlebar: Some(TitlebarOptions {
                    title: Some("A2UI // TACTICAL HUD".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                // GNOME Wayland ignores server-side decoration requests; force
                // CSD so the TitleBar below draws drag + min/max/close.
                window_decorations: Some(gpui::WindowDecorations::Client),
                ..Default::default()
            };

            cx.spawn(async move |cx| {
                cx.open_window(window_options, |window, cx| {
                    let view = cx.new(HudApp::new);
                    cx.new(|cx| Root::new(view, window, cx))
                })?;
                Ok::<_, anyhow::Error>(())
            })
            .detach();
        });
}
