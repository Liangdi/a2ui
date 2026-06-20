//! # Example: Sci-fi HUD — Bevy backend
//!
//! A neon tactical HUD rebuilt on the a2ui protocol, rendered into a real OS
//! window by Bevy's native ECS UI stack. This is the Bevy counterpart of the
//! ratatui-style [`17_scifi_hud`], the Iced [`17_scifi_hud`], and the Dioxus
//! [`17_scifi_hud`]: same data, same "the data model is the only source of
//! truth" architecture, different renderer.
//!
//! Where the ratatui version drew ASCII gauges and a character-grid radar with
//! custom `TuiComponent`s, the Iced version used `progress_bar` gauges + a
//! `Canvas`-drawn radar, and the Dioxus version built CSS-bar gauges + an SVG
//! radar, this one builds ordinary **Bevy UI nodes** directly — flex-bar
//! gauges, an ASCII radar grid, a neon status line, and a rolling event log —
//! and reads every live value from the a2ui data model. No component tree is
//! declared through the a2ui-bevy reconciler: the layout *is* the Bevy entity
//! tree. Only the **data** flows through the protocol.
//!
//! That mirrors the Bevy backend's defining strength: it is **retained ECS**.
//! The HUD entity tree is spawned once (in a `Startup` system); each frame an
//! `Update` system reads the freshly-updated data model and *mutates the
//! existing entities in place* (`Text.0`, `Node.width`, `BackgroundColor`,
//! `TextColor`) via queries — no per-frame rebuild, no `EditBuffers` bridge.
//! This is exactly the identity-preserving pattern the `a2ui-bevy` reconciler
//! itself uses; here we drive it by hand so the example depends only on `bevy`
//! + the framework-neutral `a2ui-base` processor. The only "data source" is the
//! same as the other versions: a `tick` counter that computes a fresh telemetry
//! snapshot and ships it with a single `updateDataModel` message.
//!
//! Animation is driven by a Bevy [`Timer`] resource: a repeating ~80 ms timer
//! (the Bevy equivalent of the ratatui loop's `event::poll`, the Iced
//! `Subscription`, and the Dioxus `tokio::sleep`) fires the `tick_hud` system,
//! which computes the next snapshot. A separate `update_hud` system re-applies
//! the model to the entities every frame regardless, so the HUD reflects the
//! latest snapshot as soon as it lands.
//!
//! The radar is an ASCII character grid — a deliberate echo of the *ratatui*
//! original (the Iced version drew it on a `Canvas`, the Dioxus version in
//! SVG; Bevy UI has no canvas, and overlaying free-form `Gizmos` on UI nodes is
//! fiddly, so the grid — which the bundled monospace `FiraMono` font renders
//! cleanly — is the natural fit).
//!
//! `q` / `Esc` (or the OS window-close button) to quit.
//!
//! [`17_scifi_hud`]: ../../a2ui/examples/17_scifi_hud.rs
//! [Iced `17_scifi_hud`]: ../../iced/examples/17_scifi_hud.rs
//! [Dioxus `17_scifi_hud`]: ../../dioxus/examples/17_scifi_hud.rs
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-bevy --example 17_scifi_hud --features backend
//! ```

use std::time::Duration;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResolution;
use serde_json::{Value, json};

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::data_model::DataModel;
use a2ui_tui::catalogs::basic::build_basic_catalog;

// ─── Neon palette (mirrors the ratatui/iced/dioxus versions' tokens) ─────────
// Bevy's `Color::srgb` takes sRGB 0..1 components and converts to linear, the
// same role Iced's `Color::from_rgb` / Dioxus's hex CSS play.
const CYAN: Color = Color::srgb(0.337, 0.941, 1.0);
const MAGENTA: Color = Color::srgb(1.0, 0.310, 0.847);
const GREEN: Color = Color::srgb(0.365, 1.0, 0.690);
const AMBER: Color = Color::srgb(1.0, 0.706, 0.329);
const BG: Color = Color::srgb(0.016, 0.067, 0.102);
const PANEL: Color = Color::srgb(0.024, 0.082, 0.118);
const TRACK: Color = Color::srgb(0.02, 0.07, 0.10);
const DIM: Color = Color::srgb(0.165, 0.416, 0.478);
const TEXT: Color = Color::srgb(0.722, 0.949, 1.0);

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
///
/// Held as a **`NonSend` resource**: `MessageProcessor` contains `RefCell`-backed
/// model maps that are `!Sync`, so it cannot satisfy Bevy's `Send + Sync`
/// resource requirement. Systems take `NonSendMut<HudState>` (tick) /
/// `NonSend<HudState>` (render) — single-threaded, one at a time.
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
}

/// Repeating ~80 ms timer driving the tick cadence (the Bevy analogue of the
/// ratatui loop's `event::poll`).
#[derive(Resource)]
struct TickTimer(Timer);

// ─── Marker components — tag the dynamic entities so `update_hud` finds them ──

#[derive(Component)]
struct StatusLabel;
/// On the colored gauge-bar fill `Node`.
#[derive(Component)]
struct GaugeBar(usize);
/// On the gauge value `Text`.
#[derive(Component)]
struct GaugeValue(usize);
/// On the radar grid `Text`.
#[derive(Component)]
struct RadarGrid;
#[derive(Component, Clone, Copy)]
enum Readout {
    Bearing,
    Range,
}
/// On a readout value `Text`.
#[derive(Component)]
struct ReadoutField(Readout);
/// On one event-log `Text` line (`0..6`).
#[derive(Component)]
struct EventLine(usize);

// ─── App wiring ──────────────────────────────────────────────────────────────

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "A2UI // TACTICAL HUD".into(),
            resolution: WindowResolution::new(1000, 680),
            ..default()
        }),
        ..default()
    }))
    .insert_non_send_resource(HudState::new())
    .insert_resource(TickTimer(Timer::new(
        Duration::from_millis(80),
        TimerMode::Repeating,
    )))
    .add_systems(Startup, spawn_hud)
    // Tick (data source) then render (apply) — chained so a fresh snapshot
    // lands before this frame's re-apply.
    .add_systems(Update, (tick_hud, update_hud).chain())
    .add_systems(Update, exit_on_esc);

    // Optional self-screenshot mode — compositor-independent: it reads the
    // window's render target directly via Bevy's `Screenshot` API, so it works
    // where desktop screenshot tools are blocked (e.g. locked-down GNOME
    // Wayland denies `org.gnome.Shell.Screenshot`, and X11 tools can't see
    // Wayland-native windows). Driven by `scripts/capture_bevy_screenshot.sh`,
    // which sets `A2UI_SCREENSHOT_PATH`; the app warms up a short while, captures
    // one PNG to that path, then quits. Without the env var the HUD runs
    // interactively as normal.
    if let Ok(path) = std::env::var("A2UI_SCREENSHOT_PATH") {
        app.insert_resource(CaptureRequest {
            path: std::path::PathBuf::from(path),
            frame: 0,
        })
        .add_systems(Update, capture_and_exit);
    }

    app.run();
}

// ─── Optional self-screenshot mode ───────────────────────────────────────────

/// Drives `capture_and_exit`: where to write the PNG + how many frames have run.
#[derive(Resource)]
struct CaptureRequest {
    path: std::path::PathBuf,
    frame: u32,
}

/// Warm the HUD up for a few dozen frames (so the tick has populated the data
/// model and the render reflects it), fire one screenshot to `path`, then —
/// once the async save has had time to flush — quit.
fn capture_and_exit(
    mut req: ResMut<CaptureRequest>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    req.frame += 1;
    // ~0.75 s at 60 fps — enough for several 80 ms ticks + a stable render.
    if req.frame == 45 {
        eprintln!("Capturing Bevy HUD screenshot -> {}", req.path.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(req.path.clone()));
    }
    // ~1.7 s — give the async disk write time to land, then quit.
    if req.frame == 100 {
        exit.write(AppExit::Success);
    }
}

// ─── Build the retained HUD entity tree (once) ───────────────────────────────

/// Spawn the camera + the HUD layout: a root Column → header / body Row / footer,
/// with the three panels (telemetry / scanner / events) and their dynamic
/// children tagged by marker components. The structure never changes; only the
/// tagged values are mutated each frame by `update_hud`.
fn spawn_hud(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands
        .spawn((
            Name::new("HUD Root"),
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(14.0)),
                row_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(BG),
        ))
        .with_children(|hud| {
            // ── Header ────────────────────────────────────────────────────
            hud.spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                width: Val::Percent(100.0),
                height: Val::Px(36.0),
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|h| {
                h.spawn((
                    Text::new("A2UI // TACTICAL HUD"),
                    TextFont { font_size: 18.0, ..default() },
                    TextColor(CYAN),
                ));
                // Spacer pushes the status to the right edge.
                h.spawn(Node { flex_grow: 1.0, ..default() });
                h.spawn((
                    Text::new(""),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(GREEN),
                    StatusLabel,
                ));
            });

            // ── Body: telemetry / scanner / events (flex 3:4:3) ───────────
            hud.spawn(Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                width: Val::Percent(100.0),
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|b| {
                // Telemetry panel.
                b.spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        flex_grow: 3.0,
                        padding: UiRect::all(Val::Px(10.0)),
                        row_gap: Val::Px(10.0),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor::all(CYAN),
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("[ TELEMETRY ]"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(CYAN),
                    ));
                    for (i, (label, _key)) in GAUGE_DEFS.iter().enumerate() {
                        p.spawn(Node {
                            display: Display::Flex,
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            width: Val::Percent(100.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new(format!("{label:<4}")),
                                TextFont { font_size: 14.0, ..default() },
                                TextColor(DIM),
                                Node { width: Val::Px(46.0), ..default() },
                            ));
                            // The track is a fixed-height dark node; the fill
                            // child's width is driven to `pct%` each frame.
                            row.spawn((
                                Node {
                                    display: Display::Flex,
                                    flex_grow: 1.0,
                                    height: Val::Px(14.0),
                                    ..default()
                                },
                                BackgroundColor(TRACK),
                            ))
                            .with_children(|tr| {
                                tr.spawn((
                                    Node {
                                        width: Val::Percent(0.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(DIM),
                                    GaugeBar(i),
                                ));
                            });
                            row.spawn((
                                Text::new("   0%"),
                                TextFont { font_size: 14.0, ..default() },
                                TextColor(TEXT),
                                Node { width: Val::Px(52.0), ..default() },
                                GaugeValue(i),
                            ));
                        });
                    }
                });

                // Scanner panel.
                b.spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        flex_grow: 4.0,
                        padding: UiRect::all(Val::Px(10.0)),
                        row_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor::all(CYAN),
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("[ SCANNER ]"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(CYAN),
                    ));
                    p.spawn((
                        Text::new(""),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(CYAN),
                        Node { flex_grow: 1.0, ..default() },
                        RadarGrid,
                    ));
                    p.spawn(Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|r| {
                        r.spawn((
                            Text::new("BEARING"),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(DIM),
                        ));
                        r.spawn((
                            Text::new("  0.0"),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(TEXT),
                            ReadoutField(Readout::Bearing),
                        ));
                        r.spawn(Node { width: Val::Px(16.0), ..default() });
                        r.spawn((
                            Text::new("RANGE"),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(DIM),
                        ));
                        r.spawn((
                            Text::new("    0m"),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(TEXT),
                            ReadoutField(Readout::Range),
                        ));
                    });
                });

                // Event-log panel.
                b.spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        flex_grow: 3.0,
                        padding: UiRect::all(Val::Px(10.0)),
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor::all(DIM),
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("[ EVENT LOG ]"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(DIM),
                    ));
                    for i in 0..6 {
                        p.spawn((
                            Text::new(""),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(DIM),
                            EventLine(i),
                        ));
                    }
                });
            });

            // ── Footer ────────────────────────────────────────────────────
            hud.spawn((
                Text::new("[ q/Esc · window-close ] exit   ·   a2ui-driven hud   ·   data flows via updateDataModel"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(DIM),
            ));
        });
}

// ─── The "data source": compute a snapshot + ship it via updateDataModel ─────

/// Throttled to ~80 ms by the `TickTimer` resource. Increments the tick counter,
/// computes the next telemetry snapshot (the only "data source" in the app), and
/// ships it as a single `updateDataModel` message (path `/` replaces the whole
/// model — a stand-in for a backend pushing a tick).
fn tick_hud(mut state: NonSendMut<HudState>, time: Res<Time>, mut timer: ResMut<TickTimer>) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    state.tick = state.tick.wrapping_add(1);
    let tf = state.tick as f64;

    // ASCII spinner (the bundled FiraMono subset has no em-dash, so use '-').
    let spinner = ['|', '/', '-', '\\'][((state.tick / 3) as usize) % 4];
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
    if state.tick % 18 == 0 {
        let entry = EVENT_POOL[state.next_pool % EVENT_POOL.len()];
        state.events.insert(0, entry);
        state.events.truncate(6);
        state.next_pool += 1;
    }
    let fresh = (state.tick % 12) < 4;
    let items: Vec<Value> = state
        .events
        .iter()
        .map(|(msg, level)| json!({ "msg": msg, "level": level }))
        .collect();

    push_snapshot(
        &mut state.processor,
        json!({
            "status": status,
            "gauges": gauges,
            "radar": radar,
            "events": { "fresh": fresh, "items": items },
        }),
    );
}

// ─── The "render": re-apply the data model to the retained entities ──────────

/// Read the live data model and mutate the tagged entities in place — gauges,
/// radar grid, status line, bearing/range, and the event log. Runs every frame
/// regardless (idempotent re-apply, exactly the pattern the a2ui-bevy
/// reconciler uses), so the HUD reflects the latest snapshot as soon as it
/// lands.
///
/// The data model is read into owned locals first (scoped so the borrow drops
/// before any mutation). The mutations then run through a single [`ParamSet`]:
/// every tagged entity carries a `Text`, and Bevy can't prove five separate
/// `&mut Text` queries disjoint (`With<T>` markers don't count for access
/// conflicts — that's the B0001 trap), so the `ParamSet` enforces one active
/// query at a time.
#[allow(clippy::type_complexity)]
fn update_hud(
    state: NonSend<HudState>,
    mut params: ParamSet<(
        Query<&mut Text, With<StatusLabel>>,
        Query<(&GaugeBar, &mut Node, &mut BackgroundColor)>,
        Query<(&GaugeValue, &mut Text)>,
        Query<&mut Text, With<RadarGrid>>,
        Query<(&ReadoutField, &mut Text)>,
        Query<(&EventLine, &mut Text, &mut TextColor)>,
    )>,
) {
    // ── Read the data model into owned locals (borrow dropped at block end) ──
    let (status_str, gauge_pcts, range, radar_str, bearing, fresh, items) = {
        let Some(surface) = state.processor.model.get_surface("hud") else {
            return;
        };
        let model = surface.data_model.borrow();
        let status = model
            .get("/status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pcts = GAUGE_DEFS
            .map(|(_, key)| read_num(&model, &format!("/gauges/{key}")).clamp(0.0, 100.0));
        let angle = read_num(&model, "/radar/angle");
        let range = read_num(&model, "/radar/range") as u32;
        let radar = radar_grid(angle);
        let bearing = (angle * 57.2957795) % 360.0;
        let fresh = model
            .get("/events/fresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let items: Vec<Value> = model
            .get("/events/items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        (status, pcts, range, radar, bearing, fresh, items)
    };

    // ── Apply: mutate the tagged entities in place, one ParamSet member at a time ──

    // Status line.
    if let Ok(mut t) = params.p0().single_mut() {
        t.0 = status_str;
    }
    // Gauges: bar width + color.
    for (bar, mut node, mut bg) in params.p1().iter_mut() {
        let pct = gauge_pcts[bar.0];
        node.width = Val::Percent(pct as f32);
        *bg = BackgroundColor(value_color(pct));
    }
    // Gauge value text.
    for (val, mut t) in params.p2().iter_mut() {
        t.0 = format!("{:>3.0}%", gauge_pcts[val.0]);
    }
    // Radar grid.
    if let Ok(mut t) = params.p3().single_mut() {
        t.0 = radar_str;
    }
    // Bearing / range readout.
    for (field, mut t) in params.p4().iter_mut() {
        t.0 = match field.0 {
            Readout::Bearing => format!("{bearing:5.1}"),
            Readout::Range => format!("{range:>5}m"),
        };
    }
    // Event log: newest highlighted while fresh.
    for (line, mut t, mut color) in params.p5().iter_mut() {
        if let Some(it) = items.get(line.0) {
            let msg = it.get("msg").and_then(|v| v.as_str()).unwrap_or("");
            let level = it.get("level").and_then(|v| v.as_str()).unwrap_or("");
            t.0 = format!("> {msg}");
            *color = TextColor(if line.0 == 0 && fresh {
                AMBER
            } else {
                level_color(level)
            });
        } else {
            t.0.clear();
        }
    }
}

/// Quit on `q` / `Esc` (window-close works too, via `DefaultPlugins`).
fn exit_on_esc(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::KeyQ) {
        exit.write(AppExit::Success);
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// An ASCII radar grid: an 8-spoke crosshair/diagonal grid, a rotating sweep
/// tip (`*`), and a center (`+`) — all rebuilt from `/radar/angle`. The y axis is
/// squashed 0.5 to compensate for the monospace cell's 2:1 aspect, matching the
/// ratatui original.
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
