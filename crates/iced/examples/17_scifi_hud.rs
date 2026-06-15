//! # Example: Sci-fi HUD — Iced backend
//!
//! A neon tactical HUD rebuilt on the a2ui protocol, rendered into a real OS
//! window by the Iced backend. This is the Iced counterpart of the ratatui-style
//! [`17_scifi_hud`]: same data, same "the data model is the only source of
//! truth" architecture, different renderer.
//!
//! Where the ratatui version drew ASCII gauges and a character-grid radar with
//! custom [`TuiComponent`]s, this one builds Iced [`Element`]s directly in
//! `view` — [`progress_bar`] gauges, a [`Canvas`]-drawn radar sweep, a styled
//! status line, and a rolling event log — and reads every live value from the
//! a2ui data model. No component tree is declared: the layout *is* the `view`
//! function. Only the **data** flows through the protocol.
//!
//! That mirrors the Iced backend's defining strength: it is Elm. `view(&state)`
//! is an immutable read of the model, so — unlike the egui backend (which needs
//! an `EditBuffers` bridge because the model is borrowed for the whole frame)
//! — custom widgets here resolve dynamic values to owned `f32`/`String`s and
//! hand them to stateless widgets, with no state bridge and no diffing. The
//! only "data source" is the same as the ratatui version: a `tick` counter that
//! computes a fresh telemetry snapshot and ships it with a single
//! `updateDataModel` message.
//!
//! Animation is driven by an Iced [`Subscription`]: a background thread emits a
//! [`Message::Tick`] every ~80 ms (the Iced equivalent of the ratatui loop's
//! `event::poll`), which `update` turns into the next snapshot. `view` then
//! reads the updated model and rebuilds the widget tree.
//!
//! [`17_scifi_hud`]: ../../a2ui/examples/17_scifi_hud.rs
//! [`TuiComponent`]: a2ui_tui::component_impl::TuiComponent
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-iced --example 17_scifi_hud --features backend
//! ```
//!
//! Close the window (or the OS's window-close button) to quit.

use std::time::Duration;

use serde_json::{json, Value};

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::data_model::DataModel;
use a2ui_tui::catalogs::basic::build_basic_catalog;

use iced::mouse;
use iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use iced::widget::progress_bar::{ProgressBar, Style as BarStyle};
use iced::widget::{Canvas, Space, column, container, row, text};
use iced::{
    Background, Border, Color, Element, Fill, Font, Length, Point, Rectangle,
    Subscription, Task, Theme,
};

// ─── Neon palette (mirrors the ratatui version's CSS tokens) ─────────────────

const CYAN: Color = Color::from_rgb(0.337, 0.941, 1.0);
const MAGENTA: Color = Color::from_rgb(1.0, 0.310, 0.847);
const GREEN: Color = Color::from_rgb(0.365, 1.0, 0.690);
const AMBER: Color = Color::from_rgb(1.0, 0.706, 0.329);
const BG: Color = Color::from_rgb(0.016, 0.067, 0.102);
const PANEL: Color = Color::from_rgb(0.024, 0.082, 0.118);
const DIM: Color = Color::from_rgb(0.165, 0.416, 0.478);
const TEXT: Color = Color::from_rgb(0.722, 0.949, 1.0);

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

// ─── The Elm application ──────────────────────────────────────────────────────

/// The HUD's runtime state: the a2ui processor (which owns the data model) plus
/// the tick counter and rolling event log — the only "data source" in the app.
struct HudApp {
    processor: MessageProcessor,
    tick: u32,
    /// Newest-first rolling event log.
    events: Vec<(&'static str, &'static str)>,
    /// Cursor into the event `pool` (wraps around to simulate a live feed).
    next_pool: usize,
}

/// The single interaction this app produces: a clock tick from the background
/// subscription. There are no buttons or inputs, so this is the whole enum.
#[derive(Debug, Clone)]
enum Message {
    Tick,
}

impl HudApp {
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

        Self {
            processor,
            tick: 0,
            events: EVENT_POOL[..6].to_vec(),
            next_pool: 6,
        }
    }

    /// Apply one clock tick: compute the next telemetry snapshot and ship it as a
    /// single `updateDataModel` message (path `/` replaces the whole model — a
    /// stand-in for a backend pushing a tick). Returns `Task::none()`; the Elm
    /// loop then re-renders `view`, which reads the freshly-updated model.
    fn update(&mut self, message: Message) -> Task<Message> {
        let Message::Tick = message;
        self.tick = self.tick.wrapping_add(1);
        let tf = self.tick as f64;

        // ── Compute the next snapshot (the only "data source" in the app) ──
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

        Task::none()
    }

    /// Build the HUD: a dark root, then a Column of header / body Row / footer.
    /// Every value is read straight from the data model — `view` holds no
    /// display state of its own.
    fn view(&self) -> Element<'_, Message> {
        let Some(surface) = self.processor.model.get_surface("hud") else {
            return text("No surface loaded.").into();
        };
        let model = surface.data_model.borrow();

        let header = self.render_header(&model);
        // Each panel fills its allotted FillPortion of the row; the outer
        // container assigns the weight, the inner panel fills that width.
        let body = row![
            container(self.render_telemetry(&model)).width(Length::FillPortion(3)),
            container(self.render_scanner(&model)).width(Length::FillPortion(4)),
            container(self.render_events(&model)).width(Length::FillPortion(3)),
        ]
        .spacing(10.0)
        .height(Fill);

        let footer = text("[ window-close ] exit   ·   a2ui-driven hud   ·   data flows via updateDataModel")
            .color(DIM)
            .size(11.0)
            .font(Font::MONOSPACE);

        let content = column![header, body, footer]
            .spacing(10.0)
            .width(Fill)
            .height(Fill);

        container(content)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(BG)),
                ..container::Style::default()
            })
            .padding(14.0)
            .width(Fill)
            .height(Fill)
            .into()
    }

    // ── Panels ──────────────────────────────────────────────────────────────

    /// Title bar: static title on the left, bound `/status` pinned to the right.
    fn render_header(&self, model: &DataModel) -> Element<'static, Message> {
        let title = text("⟁ A2UI // TACTICAL HUD")
            .color(CYAN)
            .size(18.0)
            .font(Font::MONOSPACE);
        let status_str = model
            .get("/status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let status = text(status_str).color(GREEN).font(Font::MONOSPACE);

        row![title, Space::new().width(Length::Fill), status]
            .align_y(iced::alignment::Vertical::Center)
            .spacing(8.0)
            .width(Fill)
            .into()
    }

    /// Telemetry panel: four neon [`progress_bar`] gauges bound to `/gauges/*`.
    fn render_telemetry(&self, model: &DataModel) -> Element<'static, Message> {
        let defs: [(&str, &str); 4] =
            [("CORE", "core"), ("PWR", "pwr"), ("HULL", "hull"), ("SHLD", "shld")];

        let mut col = column![panel_title("◈ TELEMETRY", CYAN)].spacing(10.0);
        for (label, key) in defs {
            let pct = read_num(model, &format!("/gauges/{key}")).clamp(0.0, 100.0);
            let accent = value_color(pct);

            let label_w = text(format!("{label:<4}"))
                .color(DIM)
                .font(Font::MONOSPACE)
                .width(Length::Fixed(46.0));
            let bar = ProgressBar::new(0.0..=100.0, pct as f32)
                .girth(14.0)
                .style(move |_theme: &Theme| BarStyle {
                    background: Background::Color(PANEL),
                    bar: Background::Color(accent),
                    border: Border::default().rounded(3.0),
                })
                .length(Fill);
            let value_w = text(format!("{pct:3.0}%"))
                .color(TEXT)
                .font(Font::MONOSPACE)
                .width(Length::Fixed(52.0));

            col = col.push(
                row![label_w, bar, value_w]
                    .align_y(iced::alignment::Vertical::Center)
                    .spacing(8.0)
                    .width(Fill),
            );
        }

        panel(col.height(Fill), CYAN)
    }

    /// Scanner panel: a [`Canvas`]-drawn radar sweep plus a bearing/range
    /// readout, driven by `/radar/angle` and `/radar/range`.
    fn render_scanner(&self, model: &DataModel) -> Element<'static, Message> {
        let angle = read_num(model, "/radar/angle");
        let range = read_num(model, "/radar/range") as u32;

        let radar = Canvas::new(Radar { angle: angle as f32 })
            .width(Fill)
            .height(Length::Fill);

        let bearing = (angle * 57.2957795) % 360.0;
        let readout = row![
            text("BEARING").color(DIM).size(10.0).font(Font::MONOSPACE),
            text(format!("{bearing:5.1}°")).color(TEXT).font(Font::MONOSPACE),
            Space::new().width(Length::Fixed(16.0)),
            text("RANGE").color(DIM).size(10.0).font(Font::MONOSPACE),
            text(format!("{range:>5}m")).color(TEXT).font(Font::MONOSPACE),
        ]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(6.0);

        let col = column![panel_title("◎ SCANNER", CYAN), radar, readout]
            .spacing(8.0)
            .height(Fill);

        panel(col, CYAN)
    }

    /// Event log: renders the bound `/events/items` array; the newest one is
    /// highlighted while `/events/fresh` is true.
    fn render_events(&self, model: &DataModel) -> Element<'static, Message> {
        let fresh = model.get("/events/fresh").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut col = column![panel_title("▤ EVENT LOG", DIM)].spacing(4.0);

        if let Some(arr) = model.get("/events/items").and_then(|v| v.as_array()) {
            for (i, it) in arr.iter().enumerate() {
                let msg = it.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                let level = it.get("level").and_then(|v| v.as_str()).unwrap_or("");
                let c = if i == 0 && fresh { AMBER } else { level_color(level) };
                col = col.push(text(format!("› {msg}")).color(c).font(Font::MONOSPACE));
            }
        }

        panel(col.height(Fill), DIM)
    }
}

// ─── Panel chrome (title + bordered box with a neon border) ───────────────────

/// A small section title in the panel's accent color.
fn panel_title(label: &str, color: Color) -> Element<'static, Message> {
    text(label.to_string())
        .color(color)
        .size(13.0)
        .font(Font::MONOSPACE)
        .into()
}

/// Wrap content in a dark, neon-bordered panel filling its allotted space.
fn panel<'a>(
    content: impl Into<Element<'a, Message>>,
    border_color: Color,
) -> Element<'a, Message> {
    container(content.into())
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(PANEL)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .padding(10.0)
        .width(Fill)
        .height(Fill)
        .into()
}

// ─── Radar canvas ─────────────────────────────────────────────────────────────

/// The radar `Program`: three range rings, a crosshair, a rotating sweep beam,
/// and a center pip — all drawn from `/radar/angle`. Stateless; rebuilt every
/// frame from the current angle.
struct Radar {
    angle: f32,
}

impl<Message> Program<Message> for Radar {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = frame.center();
        let radius = (bounds.width.min(bounds.height) / 2.0 - 6.0).max(1.0);

        // Range rings.
        for i in 1..=3 {
            let r = radius * i as f32 / 3.0;
            frame.stroke(
                &Path::circle(center, r),
                Stroke::default().with_color(DIM).with_width(1.0),
            );
        }

        // Crosshair.
        let h = Path::line(
            Point::new(center.x - radius, center.y),
            Point::new(center.x + radius, center.y),
        );
        let v = Path::line(
            Point::new(center.x, center.y - radius),
            Point::new(center.x, center.y + radius),
        );
        frame.stroke(&h, Stroke::default().with_color(DIM).with_width(1.0));
        frame.stroke(&v, Stroke::default().with_color(DIM).with_width(1.0));

        // Sweep beam + tip pip.
        let tip = Point::new(
            center.x + self.angle.cos() * radius,
            center.y + self.angle.sin() * radius,
        );
        frame.stroke(
            &Path::line(center, tip),
            Stroke::default().with_color(CYAN).with_width(2.0),
        );
        frame.fill(&Path::circle(tip, 3.0), CYAN);

        // Center pip.
        frame.fill(&Path::circle(center, 3.0), MAGENTA);

        vec![frame.into_geometry()]
    }
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

/// A subscription source: spawn a background thread that emits a [`Message::Tick`]
/// every ~80 ms. This is the Iced counterpart of the ratatui loop's
/// `event::poll(Duration::from_millis(80))`.
///
/// We use a plain OS thread + an unbounded mpsc channel (rather than
/// `iced::time::every`) because the backend's `thread-pool` executor exposes no
/// async timer — its `time` module is empty. The thread exits cleanly when the
/// channel's receiver is dropped (i.e. when the app closes).
fn tick_stream() -> iced::futures::channel::mpsc::UnboundedReceiver<Message> {
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(80));
        if tx.unbounded_send(Message::Tick).is_err() {
            break; // receiver dropped — app closed
        }
    });
    rx
}

fn main() -> iced::Result {
    iced::application(HudApp::new, HudApp::update, HudApp::view)
        .title(|_state: &HudApp| "A2UI // TACTICAL HUD".to_string())
        .theme(|_state: &HudApp| Theme::Dark)
        .subscription(move |_state: &HudApp| Subscription::run(tick_stream))
        .window_size(iced::Size::new(1000.0, 680.0))
        .resizable(true)
        .run()
}
