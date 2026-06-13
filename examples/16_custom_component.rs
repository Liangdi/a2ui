//! # Example: Custom Component — implementing `TuiComponent`
//!
//! Every built-in component (Text, Button, Slider, …) is just a struct that
//! implements the `TuiComponent` trait. This example walks the full path for
//! adding your *own* component end-to-end:
//!
//! 1. Define a struct and `impl TuiComponent` for it — `name`, `render`, and
//!    optionally `handle_event`.
//! 2. Insert it into the component `registry` consumed by `SurfaceRenderer`.
//!    Implementing `TuiComponent` automatically satisfies `ComponentApi`
//!    (blanket impl), so no other glue is needed.
//! 3. Reference it from JSON by its `name`. The message processor accepts
//!    unknown component types gracefully (graceful degradation), so **no
//!    catalog entry is required** — only the registry needs to know how to
//!    render it.
//!
//! The component is a `ProgressMeter`: a hand-drawn ASCII progress bar
//! `[██████░░░░] 60%` whose value is **bound to the data model** and adjusted
//! live with `←`/`→` (`±10`) — the change flows back through
//! `EventResult::DataUpdate`, exactly like the built-in interactive components.
//!
//! On top of the manual control, the example loop also **auto-advances** the
//! bound value: once per second it bumps `/progress` by a small random amount
//! until it reaches `100%`, then stops (a stand-in for a download finishing).
//! This shows a non-interactive value source driving the very same binding the
//! component renders.
//!
//! ## Run
//! ```sh
//! cargo run --example 16_custom_component
//! ```

use std::io;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use serde_json::Value;

use a2ui::core::catalog::Catalog;
use a2ui::core::event::{EventResult, InputEvent, InputKey};
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::model::component_context::ComponentContext;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui::tui::component_impl::{ComponentRegistry, TuiComponent};

// ─── The custom component ───────────────────────────────────────────────────

/// A self-contained, hand-drawn progress bar.
///
/// This is a *leaf* component: it renders itself and never delegates to
/// children, so the `render_child` closure is unused.
struct ProgressMeterComponent;

impl TuiComponent for ProgressMeterComponent {
    fn name(&self) -> &'static str {
        "ProgressMeter"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: ratatui::layout::Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, ratatui::layout::Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let Some(comp_model) = ctx.components.get(&ctx.component_id) else {
            return;
        };

        // `value` may be a literal number or a data binding `{"path": "/x"}`.
        let pct = resolve_value(ctx, comp_model.get_raw("value")).clamp(0.0, 100.0);
        let label: Option<String> = comp_model.get_property("label");

        // Reserve room for the label, the brackets, and the percentage.
        let prefix = match &label {
            Some(l) => format!("{l}  "),
            None => String::new(),
        };
        let suffix = format!(" {:3.0}%", pct);
        let reserved = prefix.chars().count() + suffix.chars().count() + 2; // +2 for [ ]
        let bar_width = (area.width as usize).saturating_sub(reserved);
        let filled = if bar_width == 0 {
            0
        } else {
            (pct / 100.0 * bar_width as f64).round() as usize
        };
        let bar = "█".repeat(filled) + &"░".repeat(bar_width - filled);

        let line = Line::from(vec![
            Span::raw(prefix),
            Span::styled(format!("[{bar}]"), Style::default().fg(Color::Cyan)),
            Span::styled(suffix, Style::default().add_modifier(Modifier::BOLD)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &InputEvent,
    ) -> Option<EventResult> {
        let InputEvent::KeyPress { key } = event;
        let step = match key {
            InputKey::Right | InputKey::Up => 10.0,
            InputKey::Left | InputKey::Down => -10.0,
            _ => return None,
        };

        let comp_model = ctx.components.get(&ctx.component_id)?;
        // Only a *bound* value can be written back — we need its path. A
        // literal value has nowhere to go, so the event is left unhandled.
        let path = comp_model
            .get_raw("value")
            .and_then(|v| v.get("path"))?
            .as_str()?
            .to_string();

        let current = resolve_value(ctx, comp_model.get_raw("value"));
        let next = (current + step).clamp(0.0, 100.0);

        Some(EventResult::DataUpdate {
            path,
            value: serde_json::json!(next),
        })
    }
}

/// Resolve a `value` property — either a literal number or `{"path": "…"}` —
/// to an `f64`, returning `0.0` when absent or unresolvable.
fn resolve_value(ctx: &ComponentContext, raw: Option<&Value>) -> f64 {
    match raw {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(v) => v
            .get("path")
            .and_then(|p| p.as_str())
            .and_then(|path| ctx.data_context.get(path))
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0),
        None => 0.0,
    }
}

// ─── Driving the component from the example loop ────────────────────────────

/// A cheap, dependency-free pseudo-random step in `3..=12`, seeded from the
/// wall clock. Good enough for a demo — pull in `rand` if you need real noise.
fn random_step() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    3.0 + (nanos % 10) as f64
}

/// Map a crossterm `KeyCode` to the framework-agnostic `InputKey`.
fn map_key(code: KeyCode) -> Option<InputKey> {
    match code {
        KeyCode::Enter => Some(InputKey::Enter),
        KeyCode::Tab => Some(InputKey::Tab),
        KeyCode::Up => Some(InputKey::Up),
        KeyCode::Down => Some(InputKey::Down),
        KeyCode::Left => Some(InputKey::Left),
        KeyCode::Right => Some(InputKey::Right),
        KeyCode::Esc => Some(InputKey::Escape),
        KeyCode::Char(' ') => Some(InputKey::Space),
        KeyCode::Char(c) => Some(InputKey::Char(c)),
        _ => None,
    }
}

/// Dispatch a key directly to the `ProgressMeter` component, returning its
/// result. This shows how to build a `ComponentContext` by hand and drive any
/// component — the same path `SurfaceRenderer` uses internally.
fn dispatch_to_meter(
    code: KeyCode,
    surface: &a2ui::core::model::surface_model::SurfaceModel,
    registry: &ComponentRegistry,
    catalog: &Catalog,
) -> Option<EventResult> {
    let key = map_key(code)?;
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();
    let comp_model = components.get("meter")?;
    let tui_comp = registry.get(&comp_model.component_type)?;
    let ctx = ComponentContext::new(
        "meter".to_string(),
        surface.id.clone(),
        &data_model,
        &components,
        &catalog.functions,
        "",
        Some("meter".to_string()),
    );
    tui_comp.handle_event(&ctx, &InputEvent::KeyPress { key })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Registry: start from the 18 built-in components, then register our
    //    own. Keyed by `TuiComponent::name`, so JSON `{"component": "..."}`
    //    is routed here automatically.
    let mut registry = build_basic_registry();
    registry.insert(
        ProgressMeterComponent.name().to_string(),
        Box::new(ProgressMeterComponent),
    );

    let render_catalog = build_basic_catalog();
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    // 2. Surface + data model (the bound value lives here).
    let create = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "demo",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "sendDataModel": true,
            "dataModel": {"progress": 40}
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create.to_string())?)?;

    // 3. Component tree. "ProgressMeter" is NOT in the basic catalog, yet it
    //    parses fine (graceful degradation) and renders through our registry.
    let update = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "demo",
            "components": [
                {"id": "root", "component": "Card", "child": "col"},
                {"id": "col", "component": "Column",
                 "children": ["title", "meter", "hint"],
                 "justify": "center", "align": "stretch"},
                {"id": "title", "component": "Text",
                 "text": "Custom Component: ProgressMeter", "variant": "h2"},
                {"id": "meter", "component": "ProgressMeter",
                 "label": "download", "value": {"path": "/progress"}},
                {"id": "hint", "component": "Text",
                 "text": "← / →  adjust  ·  q  quit", "variant": "caption"}
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update.to_string())?)?;

    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // 4. Render + interaction loop.
    let mut last_auto = Instant::now();
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(7), Constraint::Length(1)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("demo") {
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, chunks[0], Some("meter"));

                let pct = surface
                    .data_model
                    .borrow()
                    .get("/progress")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let done = pct >= 100.0;
                let status = if done { "✓ complete" } else { "downloading…" };
                let bar = Paragraph::new(Line::from(format!(
                    " q:quit   ←/→:±10   |  {status}   /progress = {pct:.0}%"
                )))
                .style(Style::default().fg(if done { Color::Green } else { Color::DarkGray }));
                frame.render_widget(bar, chunks[1]);
            }
        })?;

        // Auto-advance once per second until the meter reads 100%. The value
        // is read here, then written back through the processor — the same
        // path a background task would use to update a bound value.
        if last_auto.elapsed() >= Duration::from_secs(1) {
            last_auto = Instant::now();
            // Compute next while the surface borrow is held; release it before
            // the mutable `process_message` call below.
            let next = processor.model.get_surface("demo").and_then(|surface| {
                let cur = surface
                    .data_model
                    .borrow()
                    .get("/progress")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                (cur < 100.0).then(|| (cur + random_step()).clamp(0.0, 100.0))
            });
            if let Some(next) = next {
                let msg = serde_json::json!({
                    "version": "v1.0",
                    "updateDataModel": {
                        "surfaceId": "demo", "path": "/progress", "value": next
                    }
                });
                let _ = processor
                    .process_message(MessageProcessor::parse_message(&msg.to_string()).unwrap());
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    other => {
                        // Dispatch to the ProgressMeter, then apply its
                        // DataUpdate back through the processor.
                        let result = processor
                            .model
                            .get_surface("demo")
                            .and_then(|s| dispatch_to_meter(other, s, &registry, &render_catalog));
                        if let Some(EventResult::DataUpdate { path, value }) = result {
                            let msg = serde_json::json!({
                                "version": "v1.0",
                                "updateDataModel": {
                                    "surfaceId": "demo", "path": path, "value": value
                                }
                            });
                            let _ = processor.process_message(
                                MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                            );
                        }
                    }
                }
            }
        }
    }

    // 5. Final value.
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    if let Some(surface) = processor.model.get_surface("demo") {
        let pct = surface
            .data_model
            .borrow()
            .get("/progress")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        println!("Final /progress = {pct:.0}%");
    }
    Ok(())
}
