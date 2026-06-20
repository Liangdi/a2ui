//! # Example: Sci-fi HUD — Dioxus backend
//!
//! A neon tactical HUD rebuilt on the a2ui protocol, rendered into a real OS
//! WebView window by the Dioxus backend. This is the Dioxus counterpart of the
//! ratatui-style [`17_scifi_hud`] and the Iced [`17_scifi_hud`]: same data, same
//! "the data model is the only source of truth" architecture, different
//! renderer.
//!
//! Where the ratatui version drew ASCII gauges and a character-grid radar and
//! the Iced version used `progress_bar` gauges + a `Canvas`-drawn radar sweep,
//! this one builds ordinary **HTML + CSS + SVG** directly in `rsx!` — CSS-bar
//! gauges, an SVG radar sweep, a neon status line, and a rolling event log —
//! and reads every live value from the a2ui data model. No component tree is
//! declared: the layout *is* the `rsx!`. Only the **data** flows through the
//! protocol.
//!
//! That mirrors the Dioxus backend's defining strength: it is reactive-signals.
//! The `MessageProcessor` lives in a `Signal`; the UI is a pure read of it, so —
//! unlike the egui backend (which needs an `EditBuffers` bridge because the
//! model is borrowed for the whole frame) — here custom widgets resolve dynamic
//! values to owned `f32`/`String`s and hand them to stateless HTML, with no
//! state bridge. The only "data source" is the same as the other versions: a
//! `tick` counter that computes a fresh telemetry snapshot and ships it with a
//! single `updateDataModel` message.
//!
//! Animation is driven by a `spawn`ed async loop that `tokio::time::sleep`s ~80
//! ms between ticks (dioxus-desktop ships a tokio runtime with the `time`
//! driver). Each iteration computes the next snapshot, pushes it to the
//! processor `Signal`, and the write re-renders the subscribers — the Dioxus
//! counterpart of the Iced `Subscription` + `Message::Tick` and the ratatui
//! loop's `event::poll`. There is no `Message` enum: the signal *is* the tick
//! channel.
//!
//! [`17_scifi_hud`]: ../../a2ui/examples/17_scifi_hud.rs
//! [Iced `17_scifi_hud`]: ../../iced/examples/17_scifi_hud.rs
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-dioxus --example 17_scifi_hud --features backend
//! ```
//!
//! Close the window (or the OS's window-close button) to quit.

use std::time::Duration;

use serde_json::{Value, json};

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::data_model::DataModel;
use a2ui_tui::catalogs::basic::build_basic_catalog;

use dioxus::prelude::*;

// ─── Neon palette (mirrors the ratatui/iced versions' tokens) ────────────────
// Hex strings, used directly as CSS colors in `rsx!` inline styles + the SVG.
const CYAN: &str = "#56f0ff";
const MAGENTA: &str = "#ff4fd9";
const GREEN: &str = "#5dff71";
const AMBER: &str = "#ffb454";
const DIM: &str = "#2a6a7a";

/// Threshold color for a 0–100 gauge reading: magenta→amber→green as it climbs.
fn value_color(pct: f64) -> &'static str {
    if pct < 30.0 {
        MAGENTA
    } else if pct < 60.0 {
        AMBER
    } else {
        GREEN
    }
}

/// Color for an event-log severity level.
fn level_color(level: &str) -> &'static str {
    match level {
        "ok" => GREEN,
        "warn" => AMBER,
        "alert" => MAGENTA,
        _ => DIM,
    }
}

// ─── The reactive HUD ─────────────────────────────────────────────────────────

/// The HUD root component: owns the a2ui processor in a `Signal`, spawns the
/// ~80 ms tick loop, and renders the neon panels straight from the data model.
fn hud() -> Element {
    // The processor holds the *entire* HUD state in its data model (seeded by a
    // `createSurface`). It lives in a `Signal` so writes (from the tick loop)
    // re-render every reader below.
    let processor: Signal<MessageProcessor> = use_signal(|| build_processor());

    // Spawn the tick loop once (use_hook runs once per mount). Each iteration
    // sleeps ~80 ms, computes the next telemetry snapshot from a local tick
    // counter, and pushes it as a single `updateDataModel` message — the only
    // "data source" in the app. Writing the `Signal` re-renders `rsx!`.
    use_hook(|| {
        spawn(async move {
            // `Signal` is `Copy`; copy the handle into a `mut` local so the
            // loop can call `.write()` (it takes `&mut self`).
            let mut processor = processor;
            let mut tick: u32 = 0;
            let mut events: Vec<(&'static str, &'static str)> = EVENT_POOL[..6].to_vec();
            let mut next_pool: usize = 6;
            loop {
                tokio::time::sleep(Duration::from_millis(80)).await;
                tick = tick.wrapping_add(1);
                let tf = tick as f64;

                let spinner = ['|', '/', '—', '\\'][((tick / 3) as usize) % 4];
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

                // Prepend a fresh event every 18 ticks; keep the six most recent.
                if tick % 18 == 0 {
                    let entry = EVENT_POOL[next_pool % EVENT_POOL.len()];
                    events.insert(0, entry);
                    events.truncate(6);
                    next_pool += 1;
                }
                let fresh = (tick % 12) < 4;
                let items: Vec<Value> = events
                    .iter()
                    .map(|(msg, level)| json!({ "msg": msg, "level": level }))
                    .collect();

                let mut p = processor.write();
                push_snapshot(
                    &mut p,
                    json!({
                        "status": status,
                        "gauges": gauges,
                        "radar": radar,
                        "events": { "fresh": fresh, "items": items },
                    }),
                );
            }
        });
    });

    // Read the current snapshot out of the data model (shared borrows, dropped
    // when this function returns its `rsx!`). Pre-compute every display string
    // (gauge styles, event colors) here so the `rsx!` body holds no format
    // logic and its `for` loops need no leading `let`.
    let status: String;
    let gauges: Vec<(String, String, String)>; // (label, value%, bar-style)
    let angle: f64;
    let range: u32;
    let event_rows: Vec<(String, String)>; // (msg, color)
    {
        let p = processor.read();
        let Some(surface) = p.model.get_surface("hud") else {
            return rsx! { div { "No surface loaded." } };
        };
        let model = surface.data_model.borrow();

        status = model
            .get("/status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        gauges = [
            ("CORE", "core"),
            ("PWR", "pwr"),
            ("HULL", "hull"),
            ("SHLD", "shld"),
        ]
        .into_iter()
        .map(|(label, key)| {
            let pct = read_num(&model, &format!("/gauges/{key}")).clamp(0.0, 100.0);
            let style = format!("width:{:.0}%;background:{}", pct, value_color(pct));
            (label.to_string(), format!("{pct:>3.0}%"), style)
        })
        .collect();
        angle = read_num(&model, "/radar/angle");
        range = read_num(&model, "/radar/range") as u32;
        let fresh = model
            .get("/events/fresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        event_rows = model
            .get("/events/items")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .enumerate()
                    .map(|(i, it)| {
                        let msg = it
                            .get("msg")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let level = it.get("level").and_then(|v| v.as_str()).unwrap_or("");
                        let c = if i == 0 && fresh {
                            AMBER
                        } else {
                            level_color(level)
                        };
                        (msg, c.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    // ── Build the HUD: header / body (3 panels) / footer ─────────────────────
    let bearing = (angle * 57.2957795) % 360.0;
    let angle_deg = angle.to_degrees() % 360.0;

    rsx! {
        div { class: "hud",
            // ── Header ───────────────────────────────────────────────────
            div { class: "hud__header",
                span { class: "hud__title", style: "color:{CYAN}", "⟁ A2UI // TACTICAL HUD" }
                span { class: "hud__spacer" }
                span { class: "mono", style: "color:{GREEN}", "{status}" }
            }

            // ── Body: telemetry / scanner / events ───────────────────────
            div { class: "hud__body",
                // Telemetry panel — four neon CSS-bar gauges bound to /gauges/*.
                div { class: "panel", style: "border-color:{CYAN}",
                    div { class: "panel__title", style: "color:{CYAN}", "◈ TELEMETRY" }
                    for (label, value, bar_style) in gauges {
                        div { class: "gauge",
                            span { class: "gauge__label mono", style: "color:{DIM}", "{label}" }
                            div { class: "gauge__track",
                                div { class: "gauge__bar", style: "{bar_style}" }
                            }
                            span { class: "gauge__value mono", style: "color:#b8f2ff", "{value}" }
                        }
                    }
                }

                // Scanner panel — an SVG radar sweep + bearing/range readout.
                div { class: "panel panel--wide", style: "border-color:{CYAN}",
                    div { class: "panel__title", style: "color:{CYAN}", "◎ SCANNER" }
                    RadarSvg { angle_deg, color: CYAN, ping: MAGENTA }
                    div { class: "readout",
                        span { class: "mono", style: "color:{DIM}", "BEARING" }
                        span { class: "mono", style: "color:#b8f2ff", "{bearing:>5.1}°" }
                        span { class: "hud__spacer" }
                        span { class: "mono", style: "color:{DIM}", "RANGE" }
                        span { class: "mono", style: "color:#b8f2ff", "{range:>5}m" }
                    }
                }

                // Event log — the bound /events/items list; newest highlighted
                // while /events/fresh is true.
                div { class: "panel", style: "border-color:{DIM}",
                    div { class: "panel__title", style: "color:{DIM}", "▤ EVENT LOG" }
                    for (msg, color) in event_rows {
                        div { class: "mono", style: "color:{color}", "› {msg}" }
                    }
                }
            }

            // ── Footer ───────────────────────────────────────────────────
            div {
                class: "hud__footer mono",
                style: "color:{DIM}",
                "[ window-close ] exit   ·   a2ui-driven hud   ·   data flows via updateDataModel"
            }
        }
    }
}

/// The SVG radar: three range rings, a crosshair, a rotating sweep beam (driven
/// by `angle_deg`), and a center pip. Stateless — rebuilt every render from the
/// current angle read out of `/radar/angle`. The Iced version drew this on a
/// `Canvas`; the WebView makes a plain SVG the natural fit.
#[component]
fn RadarSvg(angle_deg: f64, color: String, ping: String) -> Element {
    let transform = format!("rotate({angle_deg:.1} 100 100)");
    rsx! {
        svg {
            class: "radar",
            view_box: "0 0 200 200",
            // Range rings.
            circle { cx: "100", cy: "100", r: "30", fill: "none", stroke: "{DIM}", "stroke-width": "1" }
            circle { cx: "100", cy: "100", r: "60", fill: "none", stroke: "{DIM}", "stroke-width": "1" }
            circle { cx: "100", cy: "100", r: "90", fill: "none", stroke: "{DIM}", "stroke-width": "1" }
            // Crosshair.
            line { x1: "10", y1: "100", x2: "190", y2: "100", stroke: "{DIM}", "stroke-width": "1" }
            line { x1: "100", y1: "10", x2: "100", y2: "190", stroke: "{DIM}", "stroke-width": "1" }
            // Sweep beam + tip pip (rotated as a group).
            g { transform: "{transform}",
                line { x1: "100", y1: "100", x2: "190", y2: "100", stroke: "{color}", "stroke-width": "2" }
                circle { cx: "190", cy: "100", r: "3", fill: "{color}" }
            }
            // Center pip.
            circle { cx: "100", cy: "100", r: "3", fill: "{ping}" }
        }
    }
}

// ─── Driving the HUD ──────────────────────────────────────────────────────────

/// Build the runtime: a processor seeded with the basic catalog, then a
/// `createSurface` carrying the *initial* data model (every value the panels
/// ever show lives under this surface's data model).
fn build_processor() -> MessageProcessor {
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
    let _ =
        processor.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap());
    processor
}

/// Build and ship one telemetry snapshot as a single `updateDataModel` message
/// (path `/` replaces the whole model — a stand-in for a backend pushing a tick).
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

// ─── The dark neon stylesheet, injected into the document head ────────────────
const STYLESHEET: &str = r#"
* { box-sizing: border-box; }
html, body { height: 100%; margin: 0; }
body { background: #04111a; }
.mono { font-family: "JetBrains Mono","SF Mono","Cascadia Code",ui-monospace,monospace; }
.hud {
  display: flex; flex-direction: column; gap: 10px;
  height: 100vh; padding: 14px;
  color: #b8f2ff; font: 13px/1.5 -apple-system,system-ui,sans-serif;
}
.hud__header, .hud__footer { display: flex; align-items: center; }
.hud__title { font-size: 18px; font-weight: 600; }
.hud__spacer { flex: 1; }
.hud__footer { font-size: 11px; }
.hud__body { display: flex; gap: 10px; flex: 1; min-height: 0; }
.panel {
  flex: 1; display: flex; flex-direction: column; gap: 8px;
  background: #06151e; border: 1px solid; border-radius: 4px; padding: 10px;
}
.panel--wide { flex: 1.34; }                 /* matches the Iced 3:4:3 FillPortion */
.panel__title { font-size: 13px; }
.gauge { display: flex; align-items: center; gap: 8px; }
.gauge__label { width: 46px; }
.gauge__track { flex: 1; height: 14px; background: #06151e; border: 1px solid #11262f; border-radius: 3px; overflow: hidden; }
.gauge__bar { height: 100%; transition: width .08s linear; }
.gauge__value { width: 52px; text-align: right; }
.readout { display: flex; align-items: center; gap: 6px; font-size: 13px; }
.radar { width: 100%; flex: 1; min-height: 0; }
"#;

fn main() {
    dioxus::LaunchBuilder::new()
        .with_cfg(desktop! {
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("A2UI // TACTICAL HUD")
                        .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(1000.0, 680.0)),
                )
                .with_custom_head(format!("<style>{STYLESHEET}</style>"))
        })
        .launch(hud);
}
