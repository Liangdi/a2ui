//! # Example: Sci-fi HUD — Slint backend
//!
//! A neon tactical HUD rebuilt on the a2ui protocol, rendered into a real OS
//! window by Slint's native renderer. This is the Slint counterpart of the
//! ratatui-style [`17_scifi_hud`], the Iced [`17_scifi_hud`], the Bevy
//! [`17_scifi_hud`], and the Dioxus [`17_scifi_hud`]: same data, same "the data
//! model is the only source of truth" architecture, different renderer.
//!
//! Where the ratatui version drew ASCII gauges and a character-grid radar with
//! custom `TuiComponent`s, the Iced version used `progress_bar` gauges + a
//! `Canvas` radar, the Bevy version built flex-bar gauges + an ASCII radar grid,
//! and the Dioxus version built CSS-bar gauges + an SVG radar, this one builds
//! the HUD with **ordinary Slint elements** — a dark panel tree (header / body
//! Row / footer), neon-bordered panels, flex-bar gauges, a monospace ASCII radar
//! grid, and a rolling event log — and reads every live value from the a2ui data
//! model. No component tree is declared through the `a2ui-slint` reconciler: the
//! layout *is* the Slint component defined inline below. Only the **data** flows
//! through the protocol.
//!
//! That mirrors the other GUI backends' examples, and sidesteps the Slint
//! backend's defining constraint — it **can't recurse** (no recursive structs nor
//! self-referential components; see [slint-ui/slint#4218]), so the gallery's
//! `live_tree` unrolls arbitrary trees into a bounded-depth node array. This HUD
//! is a fixed (non-recursive) layout, so the inline [`slint!`] macro is enough —
//! no `build.rs` codegen, no `live_tree`. The only "data source" is the same as
//! the other versions: a `tick` counter that computes a fresh telemetry snapshot
//! and ships it with a single `updateDataModel` message.
//!
//! Animation is driven by a Slint [`Timer`] in `Repeated` mode: it fires every
//! ~80 ms (the Slint equivalent of the ratatui loop's `event::poll`, the Iced
//! `Subscription`, and the Bevy `Timer` resource), which computes the next
//! snapshot, pushes it, then re-applies the model to the window's `in property`s.
//! Because Slint is **retained + reactive**, setting a property is the whole
//! "render" — there is no per-frame rebuild (unlike Iced/egui) and no manual
//! entity mutation (unlike Bevy); the engine diff-and-repaints for us.
//!
//! The radar is an ASCII character grid — a deliberate echo of the Bevy version
//! (the ratatui original drew it on the character grid; the Iced version on a
//! `Canvas`, the Dioxus version in SVG). Slint's `Path` element *could* draw a
//! vector sweep, but the monospace grid keeps this example self-contained and
//! free of geometry bookkeeping, matching the Bevy sibling.
//!
//! Close the window (OS window-close button) to quit.
//!
//! ## Screenshot
//! Compositor-independent self-capture (the Slint analog of the Bevy example's
//! `Screenshot::primary_window()` path): when `A2UI_SCREENSHOT_PATH` is set, the
//! app installs a headless Slint platform and rasterizes one frame into an
//! in-memory buffer via the software renderer — no window, no compositor — then
//! writes a PNG and exits. Driven by `scripts/capture_slint_screenshot.sh`,
//! which works where desktop screenshot tools are blocked (locked-down GNOME
//! Wayland denies `org.gnome.Shell.Screenshot`, and X11 tools can't see
//! Wayland-native windows).
//!
//! [`17_scifi_hud`]: ../../a2ui/examples/17_scifi_hud.rs
//! [Iced `17_scifi_hud`]: ../../iced/examples/17_scifi_hud.rs
//! [Bevy `17_scifi_hud`]: ../../bevy/examples/17_scifi_hud.rs
//! [Dioxus `17_scifi_hud`]: ../../dioxus/examples/17_scifi_hud.rs
//! [`slint!`]: slint::slint
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-slint --example 17_scifi_hud --features backend
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use serde_json::{Value, json};

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::data_model::DataModel;
use a2ui_tui::catalogs::basic::build_basic_catalog;

use slint::{Color, ModelRc, Timer, TimerMode, VecModel};

// ─── The HUD layout (inline .slint) ──────────────────────────────────────────
//
// Every live value is an `in property` set from Rust each tick (read straight out
// of the a2ui data model). The radar grid is a multiline `string`; the gauges and
// events are `[struct]` models carrying their own color (threshold-tinted in
// Rust). `@children` forwards each `NeonPanel` instance's children into a padded
// vertical stack, so the panel chrome is defined once.

slint::slint! {
    // One telemetry gauge: label + fill % (0–100) + preformatted "%-text" + color.
    struct GaugeDef {
        label: string,
        pct: float,
        text: string,
        color: color,
    }

    // One event-log line: the rendered text + its severity color.
    struct EventLine {
        text: string,
        color: color,
    }

    // A dark, neon-bordered panel wrapping its children in a padded column.
    component NeonPanel inherits Rectangle {
        in property <color> accent: #56f0ff;
        border-width: 1px;
        border-color: self.accent;
        border-radius: 4px;
        background: #06141f;
        VerticalLayout {
            padding: 10px;
            @children
        }
    }

    export component Hud inherits Window {
        title: "A2UI // TACTICAL HUD";
        preferred-width: 1000px;
        preferred-height: 680px;
        background: #04111a;

        in property <string> status: "SYS |  * ONLINE";
        in property <[GaugeDef]> gauges: [];
        in property <string> radar: "";
        in property <string> bearing: "  0.0";
        in property <string> range: "    0m";
        in property <[EventLine]> events: [];
        in property <string> footer:
            "[ window-close ] exit   -   a2ui-driven hud   -   data flows via updateDataModel";

        VerticalLayout {
            padding: 14px;
            spacing: 10px;

            // ── Header: title (left) + bound /status pinned right ──────────
            HorizontalLayout {
                spacing: 8px;
                Text {
                    text: "A2UI // TACTICAL HUD";
                    color: #56f0ff;
                    font-size: 18px;
                    font-weight: 800;
                    vertical-alignment: center;
                }
                Rectangle { horizontal-stretch: 1; }
                Text {
                    text: root.status;
                    color: #5dffb0;
                    font-size: 14px;
                    vertical-alignment: center;
                }
            }

            // ── Body: telemetry / scanner / events (flex 3:4:3) ────────────
            HorizontalLayout {
                spacing: 10px;
                vertical-stretch: 1;

                // Telemetry — four neon flex-bar gauges.
                NeonPanel {
                    accent: #56f0ff;
                    horizontal-stretch: 3;
                    Text { text: "[ TELEMETRY ]"; color: #56f0ff; font-size: 13px; }
                    for g in root.gauges : HorizontalLayout {
                        spacing: 8px;
                        Text {
                            text: g.label;
                            color: #2a6a7a;
                            min-width: 46px;
                            vertical-alignment: center;
                            font-size: 14px;
                        }
                        // A thin gauge bar, vertically centered in the row. Slint
                        // layouts stretch children to fill the cross-axis by
                        // default, so a bare `Rectangle` would grow to the full
                        // row height (a fat bar). Instead the cell stretches (to
                        // claim its share of the row width), and holds a fixed
                        // 12px track centered via `y`. The fill is a DIRECT child
                        // of that track (not nested in a layout) so its
                        // `parent.width`-derived width never feeds back through a
                        // layout's layoutinfo → no binding loop.
                        Rectangle {
                            horizontal-stretch: 1;
                            Rectangle {
                                height: 12px;
                                width: 100%;
                                y: (parent.height - self.height) / 2;
                                background: #051421;
                                border-radius: 6px;
                                clip: true;
                                Rectangle {
                                    height: 100%;
                                    width: parent.width * (g.pct / 100);
                                    background: g.color;
                                    border-radius: 6px;
                                }
                            }
                        }
                        Text {
                            text: g.text;
                            color: #b8f2ff;
                            min-width: 52px;
                            vertical-alignment: center;
                            font-size: 14px;
                        }
                    }
                }

                // Scanner — monospace ASCII radar grid + bearing/range readout.
                NeonPanel {
                    accent: #56f0ff;
                    horizontal-stretch: 4;
                    Text { text: "[ SCANNER ]"; color: #56f0ff; font-size: 13px; }
                    Text {
                        text: root.radar;
                        color: #56f0ff;
                        font-family: "monospace";
                        font-size: 13px;
                        wrap: no-wrap;
                        vertical-stretch: 1;
                    }
                    HorizontalLayout {
                        spacing: 6px;
                        Text { text: "BEARING"; color: #2a6a7a; font-size: 10px; vertical-alignment: center; }
                        Text { text: root.bearing; color: #b8f2ff; font-size: 13px; vertical-alignment: center; }
                        Rectangle { width: 16px; }
                        Text { text: "RANGE"; color: #2a6a7a; font-size: 10px; vertical-alignment: center; }
                        Text { text: root.range; color: #b8f2ff; font-size: 13px; vertical-alignment: center; }
                    }
                }

                // Event log — rolling list, newest tinted amber while fresh.
                NeonPanel {
                    accent: #2a6a7a;
                    horizontal-stretch: 3;
                    Text { text: "[ EVENT LOG ]"; color: #2a6a7a; font-size: 13px; }
                    for e in root.events : Text {
                        text: e.text;
                        color: e.color;
                        font-size: 13px;
                    }
                }
            }

            // ── Footer ─────────────────────────────────────────────────────
            Text {
                text: root.footer;
                color: #2a6a7a;
                font-size: 11px;
            }
        }
    }
}

// ─── Neon palette ────────────────────────────────────────────────────────────
// Only the *dynamic* colors (gauge/event tints) live here — the static panel
// chrome (BG / panel / borders / titles) is hardcoded as hex literals in the
// `.slint` above, since Slint markup can't reference Rust `const`s. These mirror
// the ratatui/iced/bevy/dioxus versions' tokens. `Color::from_rgb_u8` is a const
// fn, so the palette can live in `const`s.
const MAGENTA: Color = Color::from_rgb_u8(0xff, 0x4f, 0xd8);
const GREEN: Color = Color::from_rgb_u8(0x5d, 0xff, 0xb0);
const AMBER: Color = Color::from_rgb_u8(0xff, 0xb4, 0x54);
const DIM: Color = Color::from_rgb_u8(0x2a, 0x6a, 0x7a);

/// Threshold color for a 0–100 gauge reading: magenta→amber→green as it climbs.
fn value_color(pct: f64) -> Color {
    if pct < 30.0 {
        MAGENTA
    } else if pct < 60.0 {
        AMBER
    } else {
        GREEN
    }
}

/// Color for an event-log severity level.
fn level_color(level: &str) -> Color {
    match level {
        "ok" => GREEN,
        "warn" => AMBER,
        "alert" => MAGENTA,
        _ => DIM,
    }
}

/// The four gauges: `(label, data-model key)`.
const GAUGE_DEFS: [(&str, &str); 4] = [
    ("CORE", "core"),
    ("PWR", "pwr"),
    ("HULL", "hull"),
    ("SHLD", "shld"),
];

// ─── Runtime state ───────────────────────────────────────────────────────────

/// The HUD's runtime state: the a2ui processor (which owns the data model) plus
/// the tick counter and rolling event log — the only "data source" in the app.
struct HudState {
    processor: MessageProcessor,
    tick: u32,
    /// Newest-first rolling event log.
    events: Vec<(&'static str, &'static str)>,
    /// Cursor into the event `pool` (wraps around to simulate a live feed).
    next_pool: usize,
}

impl HudState {
    /// Build the runtime: a processor seeded with the basic catalog, then a
    /// `createSurface` carrying the *initial* data model (every value the panels
    /// ever show lives under this surface's data model).
    fn new() -> Self {
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

        let create = json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "hud",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "sendDataModel": true,
                "dataModel": {
                    "status": "SYS |  * ONLINE",
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
        let _ = processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap());

        Self {
            processor,
            tick: 0,
            events: EVENT_POOL[..6].to_vec(),
            next_pool: 6,
        }
    }

    /// Apply one clock tick: compute the next telemetry snapshot and ship it as a
    /// single `updateDataModel` message (path `/` replaces the whole model — a
    /// stand-in for a backend pushing a tick). The caller then re-applies the
    /// model to the window's properties.
    fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let tf = self.tick as f64;

        // ── Compute the next snapshot (the only "data source" in the app) ──
        // ASCII spinner + `*` status dot — Slint's bundled default font has no
        // `●`/`—` glyphs guaranteed, so we stay ASCII-safe (mirrors the Bevy
        // version's FiraMono-subset workaround).
        let spinner = ['|', '/', '-', '\\'][((self.tick / 3) as usize) % 4];
        let status = format!("SYS {spinner}  * ONLINE");
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
}

// ─── The "render": re-apply the data model to the window's properties ─────────

/// Read the live data model and set the HUD window's `in property`s — status,
/// gauges, radar grid, bearing/range, and the event log. Setting a Slint property
/// is the whole "render": the engine repaints reactively (idempotent re-apply,
/// the same shape the other backends' render passes take).
fn apply_state(hud: &Hud, state: &HudState) {
    let Some(surface) = state.processor.model.get_surface("hud") else {
        return;
    };
    let model = surface.data_model.borrow();

    hud.set_status(
        model
            .get("/status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
    );

    // Gauges — each carries its own threshold-tinted color.
    let gauges: Vec<GaugeDef> = GAUGE_DEFS
        .iter()
        .map(|(label, key)| {
            let pct = read_num(&model, &format!("/gauges/{key}")).clamp(0.0, 100.0);
            GaugeDef {
                label: (*label).into(),
                pct: pct as f32,
                text: format!("{pct:3.0}%").into(),
                color: value_color(pct),
            }
        })
        .collect();
    hud.set_gauges(ModelRc::new(Rc::new(VecModel::from(gauges))));

    // Scanner — ASCII grid rebuilt from the sweep angle, plus the readouts.
    let angle = read_num(&model, "/radar/angle");
    let range = read_num(&model, "/radar/range") as u32;
    hud.set_radar(radar_grid(angle).into());
    hud.set_bearing(format!("{:5.1}", (angle * 57.2957795) % 360.0).into());
    hud.set_range(format!("{range:>5}m").into());

    // Event log — newest highlighted while fresh.
    let fresh = model
        .get("/events/fresh")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let events: Vec<EventLine> = model
        .get("/events/items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(i, it)| {
                    let msg = it.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                    let level = it.get("level").and_then(|v| v.as_str()).unwrap_or("");
                    let color = if i == 0 && fresh {
                        AMBER
                    } else {
                        level_color(level)
                    };
                    EventLine {
                        text: format!("> {msg}").into(),
                        color,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    hud.set_events(ModelRc::new(Rc::new(VecModel::from(events))));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Optional self-screenshot mode — compositor-independent: when
    // A2UI_SCREENSHOT_PATH is set, install a headless Slint platform and render
    // one frame straight into an in-memory pixel buffer via the software
    // renderer (no window, no compositor — the Slint analog of Bevy's
    // `Screenshot::primary_window()` + `save_to_disk`), then write a PNG and
    // exit. This is what `scripts/capture_slint_screenshot.sh` drives; desktop
    // screenshot tools are blocked on a locked-down GNOME Wayland session.
    if let Ok(path) = std::env::var("A2UI_SCREENSHOT_PATH") {
        capture_screenshot(&path)?;
        return Ok(());
    }

    run()?;
    Ok(())
}

/// Open the window and run the Slint event loop until it's closed.
fn run() -> Result<(), slint::PlatformError> {
    let state = Rc::new(RefCell::new(HudState::new()));
    let hud = Hud::new()?;

    // First frame: tick once so the spinner is already moving, then apply.
    state.borrow_mut().tick();
    apply_state(&hud, &state.borrow());

    // The ~80 ms tick — the Slint equivalent of the ratatui loop's `event::poll`,
    // the Iced `Subscription`, and the Bevy `Timer` resource. Repeating: compute
    // the next snapshot, push it, then re-apply the model to the window.
    let weak = hud.as_weak();
    let timer_state = Rc::clone(&state);
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(80), move || {
        timer_state.borrow_mut().tick();
        if let Some(hud) = weak.upgrade() {
            apply_state(&hud, &timer_state.borrow());
        }
    });

    // Show the window and run the Slint event loop until it's closed. The timer
    // is a local here, so it stays armed for the whole run.
    hud.run()
}

// ─── Headless screenshot capture ─────────────────────────────────────────────

/// Render one frame of the HUD to a PNG at `path`, without a window.
///
/// Installs a custom [`Platform`] whose windows are [`MinimalSoftwareWindow`]s
/// (a `WindowAdapter` backed by the software renderer), so `Hud::new()` gets a
/// render-to-buffer target instead of a winit window. We then push one tick of
/// data, force a full redraw, and rasterize into an RGB pixel buffer via
/// [`SoftwareRenderer::render`]. `Rgb8Pixel` (re-exported from the `rgb` crate,
/// `#[repr(C)]`) is one of the [`TargetPixel`] impls the software renderer
/// accepts directly; the buffer is packed to bytes and encoded as PNG with the
/// `image` crate (already a `backend`-gated dep of this crate).
fn capture_screenshot(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};

    // Capture resolution — matches the Window's `preferred-width/height`.
    const W: u32 = 1000;
    const H: u32 = 680;

    // `NewBuffer`: the renderer treats the whole buffer as fresh and paints the
    // entire frame (ReusedBuffer would only paint the dirty region, missing the
    // first full render).
    let msw = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);

    // A platform whose every window is our shared `MinimalSoftwareWindow`. Must
    // be installed before `Hud::new()` (which asks the platform for a window).
    struct Headless(Rc<MinimalSoftwareWindow>);
    impl Platform for Headless {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(Rc::clone(&self.0) as Rc<dyn WindowAdapter>)
        }
    }
    slint::platform::set_platform(Box::new(Headless(Rc::clone(&msw))))?;

    let state = Rc::new(RefCell::new(HudState::new()));
    let hud = Hud::new()?;
    hud.window()
        .set_size(slint::LogicalSize::new(W as f32, H as f32));
    // One frame of telemetry so the HUD has live (non-default) values.
    state.borrow_mut().tick();
    apply_state(&hud, &state.borrow());

    // Force a full redraw, then rasterize into the buffer. `Rgb8Pixel` is the
    // `TargetPixel`; the stride is the buffer width in pixels.
    hud.window().request_redraw();
    let mut pixels = vec![slint::Rgb8Pixel { r: 0, g: 0, b: 0 }; (W * H) as usize];
    let drew = msw.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, W as usize);
    });
    if !drew {
        return Err("Slint reported nothing to draw — capture produced no frame".into());
    }

    // Pack RGB pixels → bytes and encode a PNG.
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    let img = image::RgbImage::from_raw(W, H, bytes)
        .ok_or("pixel buffer did not match the capture dimensions")?;
    img.save(path)?;
    eprintln!("Captured Slint HUD screenshot -> {path}");
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// An ASCII radar grid: an 8-spoke crosshair/diagonal grid, a rotating sweep
/// tip (`*`), and a center (`+`) — all rebuilt from `/radar/angle`. The y axis is
/// squashed 0.5 to compensate for the monospace cell's 2:1 aspect, matching the
/// ratatui/bevy originals.
fn radar_grid(angle: f64) -> String {
    const W: i32 = 21;
    const H: i32 = 11;
    let cx = (W - 1) as f64 / 2.0;
    let cy = (H - 1) as f64 / 2.0;
    let radius = 5.0_f64;
    let tip_x = cx + angle.cos() * radius;
    let tip_y = cy + angle.sin() * radius * 0.5;

    let mut out = String::with_capacity(((W + 1) * H) as usize);
    for y in 0..H {
        for x in 0..W {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let ch = if dx.abs() < 0.5 && dy.abs() < 0.5 {
                '+'
            } else if (x as f64 - tip_x).abs() < 0.7 && (y as f64 - tip_y).abs() < 0.7 {
                '*'
            } else if dx.abs() < 0.5 {
                '|'
            } else if dy.abs() < 0.5 {
                '-'
            } else if (dx.abs() - dy.abs()).abs() < 0.5 {
                '.'
            } else {
                ' '
            };
            out.push(ch);
        }
        out.push('\n');
    }
    out
}

/// Build and ship one telemetry snapshot as a single `updateDataModel` message.
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
