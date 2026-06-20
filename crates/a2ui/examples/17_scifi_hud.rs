//! # Example: Sci-fi HUD — a2ui-driven cyberpunk heads-up display
//!
//! A neon tactical HUD rebuilt on the a2ui protocol. Where the ratatui-style
//! version drew everything by hand from a self-incrementing `tick`, this one
//! splits the screen into an a2ui **component tree** (`Column` → header / body
//! `Row` / footer) and pushes every live value through the protocol as a
//! `updateDataModel` snapshot — gauges, the radar sweep, the status line, and a
//! rolling event log all update purely from data bindings.
//!
//! The four specialized panels (`HudHeader`, `HudTelemetry`, `HudScanner`,
//! `HudEvents`) are custom `TuiComponent`s, exactly like the `ProgressMeter` in
//! example 16. Each declares its data binding in JSON (`"source": {"path":
//! "/gauges"}`) and renders from whatever the data model currently holds — they
//! have no notion of time or randomness themselves. The main loop is the only
//! "data source": once per frame it computes a fresh telemetry snapshot and
//! ships it with a single `updateDataModel` message.
//!
//! `q` / `Esc` to quit.
//!
//! ## Run
//! ```sh
//! cargo run --example 17_scifi_hud
//! ```

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};
use serde_json::{Value, json};

use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::model::component_context::ComponentContext;
use a2ui::core::protocol::common_types::DynamicString;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui::tui::component_impl::TuiComponent;
use a2ui::tui::surface::SurfaceRenderer;

// ─── Neon palette (the CSS tokens from the ratatui-style version) ───────────

const CYAN: Color = Color::Rgb(0x56, 0xf0, 0xff);
const MAGENTA: Color = Color::Rgb(0xff, 0x4f, 0xd8);
const GREEN: Color = Color::Rgb(0x5d, 0xff, 0xb0);
const AMBER: Color = Color::Rgb(0xff, 0xb4, 0x54);
const BG: Color = Color::Rgb(0x04, 0x11, 0x1a);
const DIM: Color = Color::Rgb(0x2a, 0x6a, 0x7a);
const TEXT: Color = Color::Rgb(0xb8, 0xf2, 0xff);

/// Threshold color for a 0–100 gauge reading: red→amber→green as it climbs.
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

// ─── Binding helpers — read a property that is `{"path": "/…"}` from the model ─

/// Resolve a `{"path": …}` object property to its current JSON value.
fn bound_value(ctx: &ComponentContext, key: &str) -> Option<Value> {
    let comp = ctx.components.get(&ctx.component_id)?;
    let raw = comp.get_raw(key)?;
    let path = raw.get("path")?.as_str()?;
    ctx.data_context.get(path)
}

/// Resolve a `DynamicString` property (literal or `{"path": …}`) to text.
fn bound_string(ctx: &ComponentContext, key: &str) -> String {
    let Some(comp) = ctx.components.get(&ctx.component_id) else {
        return String::new();
    };
    match comp.get_property::<DynamicString>(key) {
        Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
        None => String::new(),
    }
}

/// Render a double-line framed panel with a title; returns the inner content area.
fn titled_panel(frame: &mut Frame, area: Rect, title: &str, border_color: Color) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(border_color))
        .title(format!(" {title} "));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

// ─── Custom components ───────────────────────────────────────────────────────

/// Title bar: static title on the left, bound `/status` on the right.
struct HudHeader;

impl TuiComponent for HudHeader {
    fn name(&self) -> &'static str {
        "HudHeader"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let title = ctx
            .components
            .get(&ctx.component_id)
            .and_then(|m| m.get_property::<String>("title"))
            .unwrap_or_default();
        let status = bound_string(ctx, "status");

        // Right-align the status in the space left after the title so it pins
        // to the right edge, whatever the terminal width.
        let title_span = format!(" {title}");
        let field = (area.width as usize)
            .saturating_sub(title_span.chars().count())
            .max(1);
        let right = format!("{:>width$}", status, width = field);
        let line = Line::from(vec![Span::raw(title_span), Span::raw(right)]);
        frame.render_widget(
            Paragraph::new(line).style(
                Style::default()
                    .fg(CYAN)
                    .add_modifier(Modifier::BOLD)
                    .bg(BG),
            ),
            area,
        );
    }

    /// Always three rows tall.
    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        Some(3)
    }
}

/// Telemetry panel: four adaptive ASCII gauges bound to `/gauges/*`.
struct HudTelemetry;

impl TuiComponent for HudTelemetry {
    fn name(&self) -> &'static str {
        "HudTelemetry"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let Some(gauges) = bound_value(ctx, "source") else {
            return;
        };
        let inner = titled_panel(frame, area, "◈ TELEMETRY", CYAN);

        let defs: [(&str, &str); 4] = [
            ("CORE", "core"),
            ("PWR", "pwr"),
            ("HULL", "hull"),
            ("SHLD", "shld"),
        ];
        let mut lines: Vec<Line<'_>> = Vec::new();
        for (label, key) in defs {
            let pct = gauges
                .get(key)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .clamp(0.0, 100.0);
            let col = value_color(pct);

            // The bar fills whatever width is left after the label and percent.
            let prefix = format!(" {label:<4} ");
            let suffix = format!(" {:3.0}%", pct);
            let reserved = prefix.chars().count() + suffix.chars().count();
            let bar_width = (inner.width as usize).saturating_sub(reserved);
            let filled = (pct / 100.0 * bar_width as f64).round() as usize;
            let bar = "▰".repeat(filled) + &"▱".repeat(bar_width - filled);

            lines.push(Line::from(vec![
                Span::raw(prefix).style(Style::default().fg(DIM)),
                Span::raw(bar).style(Style::default().fg(col)),
                Span::raw(suffix).style(Style::default().fg(TEXT)),
            ]));
        }
        frame.render_widget(List::new(lines), inner);
    }
}

/// Scanner panel: a sweeping radar grid plus a bearing/range readout, driven by
/// `/radar/angle` and `/radar/range`.
struct HudScanner;

impl TuiComponent for HudScanner {
    fn name(&self) -> &'static str {
        "HudScanner"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let Some(radar) = bound_value(ctx, "source") else {
            return;
        };
        let angle = radar.get("angle").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let range = radar.get("range").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;

        let inner = titled_panel(frame, area, "◎ SCANNER", CYAN);
        let scan = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(inner);

        // Radar grid.
        let grid_style = Style::default().fg(CYAN);
        let target_style = Style::default().fg(MAGENTA);
        let ring_style = Style::default().fg(DIM);
        let width = scan[0].width.max(2) as usize;
        let height = scan[0].height as usize;
        let (cx, cy) = ((width as i32) / 2, (height as i32) / 2);
        let radius = (cx.min(cy) - 1).max(1) as f64;

        let tip_x = cx as f64 + angle.cos() * radius;
        let tip_y = cy as f64 + angle.sin() * radius * 0.5; // squash for char aspect

        let mut grid_lines: Vec<Line<'_>> = Vec::new();
        for y in 0..height {
            let mut spans: Vec<Span<'_>> = Vec::new();
            for x in 0..width {
                let (dx, dy) = (x as i32 - cx, y as i32 - cy);
                let (ch, st) = if dx == 0 && dy == 0 {
                    ('✛', target_style)
                } else if (x as f64 - tip_x).abs() < 0.6 && (y as f64 - tip_y).abs() < 0.6 {
                    ('●', grid_style)
                } else if dx.abs() == dy.abs() && dx.abs() == (radius as i32).max(1) {
                    ('·', ring_style)
                } else {
                    (' ', Style::default())
                };
                spans.push(Span::styled(ch.to_string(), st));
            }
            grid_lines.push(Line::from(spans));
        }
        frame.render_widget(List::new(grid_lines), scan[0]);

        // Bearing / range readout.
        let bearing = (angle * 57.2958) % 360.0;
        let label = Style::default().fg(DIM);
        let value = Style::default().fg(TEXT);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" BEARING ").style(label),
                Span::raw(format!("{bearing:5.1}°")).style(value),
                Span::raw("   RANGE ").style(label),
                Span::raw(format!("{range:4}m")).style(value),
            ]))
            .alignment(Alignment::Center),
            scan[1],
        );
    }
}

/// Event log: renders a bound array of `{msg, level}` items, the newest one
/// highlighted while `/events/fresh` is true.
struct HudEvents;

impl TuiComponent for HudEvents {
    fn name(&self) -> &'static str {
        "HudEvents"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let Some(events) = bound_value(ctx, "source") else {
            return;
        };
        let fresh = events
            .get("fresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let inner = titled_panel(frame, area, "▤ EVENT LOG", DIM);

        let mut items: Vec<ListItem<'_>> = Vec::new();
        if let Some(arr) = events.get("items").and_then(|v| v.as_array()) {
            for (i, it) in arr.iter().enumerate() {
                let msg = it.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                let level = it.get("level").and_then(|v| v.as_str()).unwrap_or("");
                let col = if i == 0 && fresh {
                    AMBER
                } else {
                    level_color(level)
                };
                items.push(ListItem::new(
                    Line::from(format!(" › {msg}")).style(Style::default().fg(col)),
                ));
            }
        }
        frame.render_widget(List::new(items), inner);
    }
}

/// One-line footer with a static hint.
struct HudFooter;

impl TuiComponent for HudFooter {
    fn name(&self) -> &'static str {
        "HudFooter"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let text = ctx
            .components
            .get(&ctx.component_id)
            .and_then(|m| m.get_property::<String>("text"))
            .unwrap_or_default();
        frame.render_widget(
            Paragraph::new(Line::from(format!(" {text}"))).style(Style::default().fg(DIM)),
            area,
        );
    }

    /// Always one row tall.
    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        Some(1)
    }
}

// ─── Driving the HUD from the example loop ───────────────────────────────────

/// Build and ship one telemetry snapshot as a single `updateDataModel` message
/// (path `/` replaces the whole model — a stand-in for a backend pushing a tick).
fn push_snapshot(processor: &mut MessageProcessor, snapshot: Value) {
    let msg = json!({
        "version": "v1.0",
        "updateDataModel": { "surfaceId": "hud", "path": "/", "value": snapshot }
    });
    let _ = processor.process_message(MessageProcessor::parse_message(&msg.to_string()).unwrap());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Registry: 18 built-ins plus our five HUD panels. Custom components are
    //    keyed by `TuiComponent::name`, so JSON `{"component": "HudScanner"}`
    //    is routed here automatically (no catalog entry needed).
    let mut registry = build_basic_registry();
    for comp in [
        Box::new(HudHeader) as Box<dyn TuiComponent>,
        Box::new(HudTelemetry),
        Box::new(HudScanner),
        Box::new(HudEvents),
        Box::new(HudFooter),
    ] {
        registry.insert(comp.name().to_string(), comp);
    }

    let render_catalog = build_basic_catalog();
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    // 2. Surface + initial data model. Every value the panels show lives here.
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
    processor.process_message(MessageProcessor::parse_message(&create.to_string())?)?;

    // 3. Component tree. The body is a Row of three weighted panels; the custom
    //    panels bind their `source` to a slice of the data model above.
    let update = json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "hud",
            "components": [
                { "id": "root", "component": "Column", "children": ["header", "body", "footer"] },
                { "id": "header", "component": "HudHeader",
                  "title": "⟁ A2UI // TACTICAL HUD", "status": { "path": "/status" } },
                { "id": "body", "component": "Row",
                  "children": ["telemetry", "scanner", "events"] },
                { "id": "telemetry", "component": "HudTelemetry", "weight": 3,
                  "source": { "path": "/gauges" } },
                { "id": "scanner", "component": "HudScanner", "weight": 4,
                  "source": { "path": "/radar" } },
                { "id": "events", "component": "HudEvents", "weight": 3,
                  "source": { "path": "/events" } },
                { "id": "footer", "component": "HudFooter",
                  "text": "[q] exit   ·   a2ui-driven hud   ·   data flows via updateDataModel" }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update.to_string())?)?;

    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // Event pool: a new line is prepended every few ticks to simulate a live feed.
    let pool: &[(&str, &str)] = &[
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
    // Newest first; start with the first six entries.
    let mut events: Vec<(&str, &str)> = pool[..6].to_vec();
    let mut next_pool = 6usize;

    let mut tick: u32 = 0;
    loop {
        tick = tick.wrapping_add(1);
        let tf = tick as f64;

        // ── Compute the next snapshot (the only "data source" in the app) ──
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
            let (msg, level) = pool[next_pool % pool.len()];
            events.insert(0, (msg, level));
            events.truncate(6);
            next_pool += 1;
        }
        let fresh = (tick % 12) < 4;
        let items: Vec<Value> = events
            .iter()
            .map(|(msg, level)| json!({ "msg": msg, "level": level }))
            .collect();

        push_snapshot(
            &mut processor,
            json!({
                "status": status,
                "gauges": gauges,
                "radar": radar,
                "events": { "fresh": fresh, "items": items },
            }),
        );

        // ── Render: fill the background, then let the component tree draw ─
        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
            if let Some(surface) = processor.model.get_surface("hud") {
                let renderer = SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, area, None);
            }
        })?;

        if event::poll(Duration::from_millis(80))?
            && let Event::Key(key) = event::read()?
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}
