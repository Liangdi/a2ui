//! # Example: Sci-fi HUD — egui backend
//!
//! A neon tactical HUD rebuilt on the a2ui protocol, rendered into a real OS
//! window by the egui backend. This is the egui counterpart of the ratatui-style
//! [`17_scifi_hud`], the Iced [`17_scifi_hud`], the Bevy [`17_scifi_hud`], and
//! the Dioxus [`17_scifi_hud`]: same data, same "the data model is the only
//! source of truth" architecture, different renderer.
//!
//! Where the ratatui version drew ASCII gauges and a character-grid radar with
//! custom [`TuiComponent`]s, the Iced version used `progress_bar` gauges + a
//! `Canvas`-drawn radar, the Bevy version built flex-bar gauges + an ASCII radar
//! grid, and the Dioxus version built CSS-bar gauges + an SVG radar, this one
//! builds ordinary **egui widgets** directly — [`ProgressBar`] gauges, a
//! [`Painter`]-drawn radar sweep, a neon status line, and a rolling event log —
//! and reads every live value from the a2ui data model. No component tree is
//! declared through the a2ui-egui walker: the layout *is* the `update` function.
//! Only the **data** flows through the protocol.
//!
//! That mirrors the egui backend's defining strength: it is **immediate mode**.
//! `update` walks the model fresh every frame, resolving dynamic values to owned
//! `f64`/`String`s and handing them to stateless egui widgets, then rebuilds the
//! whole widget tree. (The gallery host, which *does* drive the generic walker,
//! keeps an `EditBuffers` bridge so the model can stay borrowed for the whole
//! frame; here we sidestep that by reading owned values up front.) The only
//! "data source" is the same as every other version: a `tick` counter that
//! computes a fresh telemetry snapshot and ships it with a single
//! `updateDataModel` message.
//!
//! Animation is driven by egui's frame clock: `update` throttles a `tick` to
//! ~80 ms (the egui equivalent of the ratatui loop's `event::poll`, the Iced
//! `Subscription`, the Bevy `Timer`, and the Dioxus `tokio::sleep`) using
//! `ctx.input(|i| i.time)`, then `ctx.request_repaint_after` keeps the loop
//! alive between input events.
//!
//! `q` / `Esc` (or the OS window-close button) to quit.
//!
//! [`17_scifi_hud`]: ../../../a2ui/examples/17_scifi_hud.rs
//! [Iced `17_scifi_hud`]: ../../iced/examples/17_scifi_hud.rs
//! [Bevy `17_scifi_hud`]: ../../bevy/examples/17_scifi_hud.rs
//! [Dioxus `17_scifi_hud`]: ../../dioxus/examples/17_scifi_hud.rs
//! [`TuiComponent`]: a2ui_tui::component_impl::TuiComponent
//! [`ProgressBar`]: eframe::egui::ProgressBar
//! [`Painter`]: eframe::egui::Painter
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-egui --example 17_scifi_hud --features backend
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Color32, ColorImage, FontId, Frame, Layout, Pos2, RichText, Sense, Stroke, Vec2,
    ViewportCommand,
};
use serde_json::{Value, json};

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::data_model::DataModel;
use a2ui_tui::catalogs::basic::build_basic_catalog;

// ─── Neon palette (mirrors the ratatui/iced/bevy/dioxus versions' tokens) ────

const CYAN: Color32 = Color32::from_rgb(0x56, 0xf0, 0xff);
const MAGENTA: Color32 = Color32::from_rgb(0xff, 0x4f, 0xd8);
const GREEN: Color32 = Color32::from_rgb(0x5d, 0xff, 0xb0);
const AMBER: Color32 = Color32::from_rgb(0xff, 0xb4, 0x54);
const BG: Color32 = Color32::from_rgb(0x04, 0x11, 0x1a);
const PANEL: Color32 = Color32::from_rgb(0x06, 0x15, 0x1e);
const DIM: Color32 = Color32::from_rgb(0x2a, 0x6a, 0x7a);
const TEXT: Color32 = Color32::from_rgb(0xb8, 0xf2, 0xff);

/// Threshold color for a 0–100 gauge reading: magenta→amber→green as it climbs.
fn value_color(pct: f64) -> Color32 {
    if pct < 30.0 {
        MAGENTA
    } else if pct < 60.0 {
        AMBER
    } else {
        GREEN
    }
}

/// Color for an event-log severity level.
fn level_color(level: &str) -> Color32 {
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

/// Monospace rich text at `size`, in `color`.
fn mono(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text)
        .font(FontId::monospace(size))
        .color(color)
}

/// A dark, neon-bordered panel `Frame` filling its allotted space.
fn panel_frame(border: Color32) -> Frame {
    Frame::NONE
        .fill(PANEL)
        .stroke((1.0, border))
        .corner_radius(4.0)
        .inner_margin(10.0)
}

// ─── The immediate-mode application ───────────────────────────────────────────

/// The HUD's runtime state: the a2ui processor (which owns the data model) plus
/// the tick clock and rolling event log — the only "data source" in the app.
struct HudApp {
    processor: MessageProcessor,
    tick: u32,
    /// Wall-clock seconds of the last ~80 ms tick (from `ctx.input(|i| i.time)`).
    last_tick: f64,
    /// Newest-first rolling event log.
    events: Vec<(&'static str, &'static str)>,
    /// Cursor into the event `pool` (wraps around to simulate a live feed).
    next_pool: usize,
    /// Optional self-screenshot mode (see `CaptureState`); `None` runs interactively.
    capture: Option<CaptureState>,
}

/// Drives the in-app screenshot: where to write the PNG + how many frames have
/// run. See `HudApp::maybe_capture`.
struct CaptureState {
    path: PathBuf,
    frames: u32,
    requested: bool,
    done: bool,
}

impl HudApp {
    /// Build the runtime: a processor seeded with the basic catalog, then a
    /// `createSurface` carrying the *initial* data model (every value the panels
    /// ever show lives under this surface's data model), then one tick so the
    /// spinner is already moving on the first frame.
    fn new(capture: Option<CaptureState>) -> Self {
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

        let mut app = Self {
            processor,
            tick: 0,
            last_tick: 0.0,
            events: EVENT_POOL[..6].to_vec(),
            next_pool: 6,
            capture,
        };
        // First frame: tick once so live (non-default) values are already in the
        // model — matches the Slint version's "tick once before the first draw".
        app.tick();
        app
    }

    /// Advance the tick by one step: compute the next telemetry snapshot (the
    /// only "data source" in the app) and ship it as a single `updateDataModel`
    /// message (path `/` replaces the whole model — a stand-in for a backend
    /// pushing a tick). `update` then re-reads the model and rebuilds the UI.
    fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let tf = self.tick as f64;

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

    /// Read the live data model into owned display values (the borrow is dropped
    /// at the end of the block, so the widget pass below holds no model borrow).
    fn read_snapshot(&self) -> Snapshot {
        let Some(surface) = self.processor.model.get_surface("hud") else {
            return Snapshot::default();
        };
        let model = surface.data_model.borrow();
        let status = model
            .get("/status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let gauges = GAUGE_DEFS
            .map(|(_, key)| read_num(&model, &format!("/gauges/{key}")).clamp(0.0, 100.0));
        let angle = read_num(&model, "/radar/angle");
        let range = read_num(&model, "/radar/range") as u32;
        let fresh = model
            .get("/events/fresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let items = model
            .get("/events/items")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|it| {
                        let msg = it
                            .get("msg")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let level = it
                            .get("level")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        (msg, level)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Snapshot {
            status,
            gauges,
            angle,
            range,
            fresh,
            items,
        }
    }

    /// In-app, compositor-independent screenshot: after the HUD has warmed up a
    /// few frames (so the tick has populated the data model and the render
    /// reflects it), request one screenshot via egui's `ViewportCommand`. The
    /// glow backend reads the GPU framebuffer *after* painting (the egui analog
    /// of Bevy's `Screenshot::primary_window()` + `save_to_disk`), so it works
    /// where desktop screenshot tools are blocked (e.g. locked-down GNOME
    /// Wayland denies `org.gnome.Shell.Screenshot`, and X11 tools can't see
    /// Wayland-native windows). The image returns next frame as an
    /// `Event::Screenshot`; we rasterize it to a PNG at `path` and exit. Driven
    /// by `scripts/capture_egui_screenshot.sh`, which sets `A2UI_SCREENSHOT_PATH`.
    fn maybe_capture(&mut self, ctx: &egui::Context) {
        let Some(cap) = self.capture.as_mut() else {
            return;
        };
        if cap.done {
            return;
        }
        cap.frames += 1;

        // The screenshot event returns on a later frame's input. egui may
        // surface it under either `raw.events` or `events`, so scan both.
        let find_shot = |events: &[egui::Event]| -> Option<Arc<ColorImage>> {
            events.iter().rev().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(Arc::clone(image))
                } else {
                    None
                }
            })
        };
        if let Some(image) =
            ctx.input(|i| find_shot(&i.raw.events).or_else(|| find_shot(&i.events)))
        {
            match save_colorimage(&image, &cap.path) {
                Ok(()) => {
                    eprintln!("Captured egui HUD screenshot -> {}", cap.path.display());
                }
                Err(err) => eprintln!("ERROR writing {}: {err}", cap.path.display()),
            }
            cap.done = true;
            std::process::exit(0);
        }

        // Warm up (~0.6 s at 60 fps — several 80 ms ticks + a stable render),
        // then request exactly one screenshot.
        if cap.frames == 40 && !cap.requested {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
            cap.requested = true;
        }

        // Safety valve: if the reply never arrives, give up loudly.
        if cap.frames > 300 {
            eprintln!(
                "ERROR: screenshot reply never arrived for {}",
                cap.path.display()
            );
            std::process::exit(1);
        }
    }
}

/// Owned display values read from the data model each frame.
struct Snapshot {
    status: String,
    gauges: [f64; 4],
    angle: f64,
    range: u32,
    fresh: bool,
    items: Vec<(String, String)>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            status: String::new(),
            gauges: [0.0; 4],
            angle: 0.0,
            range: 0,
            fresh: false,
            items: Vec::new(),
        }
    }
}

impl eframe::App for HudApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Terminal-HUD look: dark visuals + monospace as the default font.
        ctx.set_visuals(egui::Visuals::dark());
        ctx.global_style_mut(|s| s.override_font_id = Some(FontId::monospace(14.0)));

        // Throttle a tick to ~80 ms on egui's frame clock, then keep repainting
        // so the animation continues between input events.
        let now = ctx.input(|i| i.time);
        if now - self.last_tick >= 0.080 {
            self.last_tick = now;
            self.tick();
        }
        ctx.request_repaint_after(Duration::from_millis(80));

        // Screenshot mode runs before we draw (and may exit this frame).
        self.maybe_capture(ctx);

        // Quit on `q` / `Esc`.
        if ctx.input(|i| i.key_pressed(egui::Key::Q) || i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.016, 0.067, 0.102, 1.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let snap = self.read_snapshot();

        // The root `Ui` eframe hands us spans the window with no background;
        // paint the dark BG across it, then lay the HUD out inside a margin.
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, BG);
        // The window can be 0-sized for the first few frames (before the
        // compositor configures it); egui asserts on negative layout sizes, so
        // skip the layout pass until there's room to draw.
        if full.width() < 20.0 || full.height() < 20.0 {
            return;
        }
        let content = full.shrink(14.0);
        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;

            render_header(ui, &snap);

            // Body fills the space between the header and the footer.
            let footer_h = 20.0;
            let body_h = (ui.available_height() - footer_h).max(120.0);
            ui.allocate_ui(Vec2::new(ui.available_width(), body_h), |body| {
                body.horizontal_top(|h| {
                    h.spacing_mut().item_spacing.x = 10.0;
                    let total = h.available_width();
                    let usable = (total - 20.0).max(1.0);
                    let (wt, ws, we) = (usable * 0.3, usable * 0.4, usable * 0.3);

                    // `allocate_ui` inherits the parent's layout (here horizontal),
                    // so each panel must opt back into a vertical layout or its
                    // rows would collapse onto one line.
                    let panel = Layout::top_down(egui::Align::LEFT);
                    h.allocate_ui_with_layout(Vec2::new(wt, body_h), panel, |p| {
                        render_telemetry(p, &snap)
                    });
                    h.allocate_ui_with_layout(Vec2::new(ws, body_h), panel, |p| {
                        render_scanner(p, &snap)
                    });
                    h.allocate_ui_with_layout(Vec2::new(we, body_h), panel, |p| {
                        render_events(p, &snap)
                    });
                });
            });

            render_footer(ui);
        });
    }
}

// ─── Panels ──────────────────────────────────────────────────────────────────

/// Title bar: static title on the left, bound `/status` pinned to the right.
fn render_header(ui: &mut egui::Ui, snap: &Snapshot) {
    ui.horizontal(|row| {
        row.label(mono("A2UI // TACTICAL HUD", 18.0, CYAN));
        row.with_layout(Layout::right_to_left(egui::Align::Center), |right| {
            right.label(mono(snap.status.clone(), 14.0, GREEN));
        });
    });
}

/// Telemetry panel: four neon [`ProgressBar`] gauges bound to `/gauges/*`.
fn render_telemetry(ui: &mut egui::Ui, snap: &Snapshot) {
    let frame = panel_frame(CYAN);
    frame.show(ui, |ui| {
        ui.set_min_height(ui.available_height());
        ui.label(mono("[ TELEMETRY ]", 13.0, CYAN));
        ui.add_space(4.0);
        for (i, (label, _key)) in GAUGE_DEFS.iter().enumerate() {
            let pct = snap.gauges[i];
            let accent = value_color(pct);
            ui.horizontal(|row| {
                row.label(mono(format!("{label:<4}"), 14.0, DIM));
                row.add_sized(
                    Vec2::new((row.available_width() - 52.0).max(1.0), 14.0),
                    egui::ProgressBar::new((pct / 100.0) as f32)
                        .fill(accent)
                        .desired_height(14.0)
                        .corner_radius(3.0)
                        .text(""),
                );
                row.label(mono(format!("{pct:3.0}%"), 14.0, TEXT));
            });
        }
    });
}

/// Scanner panel: a [`Painter`]-drawn radar sweep plus a bearing/range readout,
/// driven by `/radar/angle` and `/radar/range`.
fn render_scanner(ui: &mut egui::Ui, snap: &Snapshot) {
    let frame = panel_frame(CYAN);
    frame.show(ui, |ui| {
        ui.set_min_height(ui.available_height());
        ui.vertical_centered(|c| c.label(mono("[ SCANNER ]", 13.0, CYAN)));

        // Radar grid: take the available square and draw rings + sweep on it.
        let avail = (ui.available_width().min(ui.available_height()) - 6.0).max(40.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(avail), Sense::hover());
        let painter = ui.painter_at(rect);
        paint_radar(&painter, rect, snap.angle as f32);

        ui.add_space(2.0);
        let bearing = (snap.angle * 57.2957795) % 360.0;
        ui.horizontal(|row| {
            row.label(mono("BEARING", 10.0, DIM));
            row.label(mono(format!("{bearing:5.1}"), 13.0, TEXT));
            row.add_space(8.0);
            row.label(mono("RANGE", 10.0, DIM));
            row.label(mono(format!("{:>5}m", snap.range), 13.0, TEXT));
        });
    });
}

/// Event log: renders the bound `/events/items` array; the newest one is
/// highlighted while `/events/fresh` is true.
fn render_events(ui: &mut egui::Ui, snap: &Snapshot) {
    let frame = panel_frame(DIM);
    frame.show(ui, |ui| {
        ui.set_min_height(ui.available_height());
        ui.label(mono("[ EVENT LOG ]", 13.0, DIM));
        ui.add_space(2.0);
        for (i, (msg, level)) in snap.items.iter().enumerate() {
            let color = if i == 0 && snap.fresh {
                AMBER
            } else {
                level_color(level)
            };
            ui.label(mono(format!("> {msg}"), 13.0, color));
        }
    });
}

/// One-line footer with a static hint.
fn render_footer(ui: &mut egui::Ui) {
    ui.label(mono(
        "[ q/Esc · window-close ] exit   ·   a2ui-driven hud   ·   data flows via updateDataModel",
        11.0,
        DIM,
    ));
}

/// The radar: three range rings, a crosshair, a rotating sweep beam + tip pip,
/// and a center pip — all drawn from the current angle. egui's y grows downward,
/// so the beam sweeps clockwise; stateless, rebuilt every frame.
fn paint_radar(painter: &egui::Painter, rect: egui::Rect, angle: f32) {
    let center = rect.center();
    let radius = (rect.width().min(rect.height()) / 2.0 - 6.0).max(1.0);

    // Range rings.
    for i in 1..=3 {
        painter.circle_stroke(center, radius * i as f32 / 3.0, Stroke::new(1.0, DIM));
    }
    // Crosshair.
    painter.line_segment(
        [
            Pos2::new(center.x - radius, center.y),
            Pos2::new(center.x + radius, center.y),
        ],
        Stroke::new(1.0, DIM),
    );
    painter.line_segment(
        [
            Pos2::new(center.x, center.y - radius),
            Pos2::new(center.x, center.y + radius),
        ],
        Stroke::new(1.0, DIM),
    );
    // Sweep beam + tip pip.
    let tip = Pos2::new(
        center.x + angle.cos() * radius,
        center.y + angle.sin() * radius,
    );
    painter.line_segment([center, tip], Stroke::new(2.0, CYAN));
    painter.circle_filled(tip, 3.0, CYAN);
    // Center pip.
    painter.circle_filled(center, 3.0, MAGENTA);
}

// ─── Driving the HUD ──────────────────────────────────────────────────────────

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

/// Encode an egui `ColorImage` (RGBA, already top-down — egui's `read_screen_rgba`
/// flips the GL read) to a PNG at `path`.
fn save_colorimage(
    image: &ColorImage,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let [w, h] = image.size;
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    image::RgbaImage::from_raw(w as u32, h as u32, bytes)
        .ok_or("pixel buffer did not match the capture dimensions")?
        .save(path)?;
    Ok(())
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

fn main() -> eframe::Result {
    // Optional self-screenshot mode (env-gated; driven by
    // scripts/capture_egui_screenshot.sh): capture one PNG to `path`, then exit.
    let capture = std::env::var("A2UI_SCREENSHOT_PATH")
        .ok()
        .map(|p| CaptureState {
            path: PathBuf::from(p),
            frames: 0,
            requested: false,
            done: false,
        });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 680.0])
            .with_title("A2UI // TACTICAL HUD"),
        ..Default::default()
    };
    eframe::run_native(
        "A2UI egui HUD",
        options,
        Box::new(move |_cc| Ok(Box::new(HudApp::new(capture)))),
    )
}
