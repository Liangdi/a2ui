//! # Example: Interactive DateTimeInput
//!
//! Demonstrates the `DateTimeInput` component across its three modes. Focus a
//! field with `Tab`, then use the arrow keys to adjust the bound value — the
//! change flows back through `EventResult::DataUpdate` and is reflected live
//! in the data model (and the status bar).
//!
//! | Mode | `enableDate` / `enableTime` | `↑`/`↓` | `←`/`→` |
//! |------|----------------------------|---------|---------|
//! | Date + Time | true / true | ±1 day | ±1 hour |
//! | Date only | true / false | ±1 day | ±1 month |
//! | Time only | false / true | ±1 minute | ±1 hour |
//!
//! ## Run
//! ```sh
//! cargo run --example 15_date_time_input
//! ```

use std::io;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::Paragraph,
};

use a2ui::core::catalog::Catalog;
use a2ui::core::event::{EventResult, InputEvent, InputKey};
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::model::component_context::ComponentContext;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui::tui::focus_manager::FocusManager;

/// Map a crossterm `KeyCode` to the framework-agnostic `InputKey`.
fn map_key(code: KeyCode) -> Option<InputKey> {
    match code {
        KeyCode::Enter => Some(InputKey::Enter),
        KeyCode::Tab => Some(InputKey::Tab),
        KeyCode::BackTab => Some(InputKey::BackTab),
        KeyCode::Up => Some(InputKey::Up),
        KeyCode::Down => Some(InputKey::Down),
        KeyCode::Left => Some(InputKey::Left),
        KeyCode::Right => Some(InputKey::Right),
        KeyCode::Backspace => Some(InputKey::Backspace),
        KeyCode::Delete => Some(InputKey::Delete),
        KeyCode::Esc => Some(InputKey::Escape),
        KeyCode::Char(' ') => Some(InputKey::Space),
        KeyCode::Char(c) => Some(InputKey::Char(c)),
        _ => None,
    }
}

/// Dispatch a keyboard event to the focused component, returning its result.
fn dispatch_to_focused(
    code: KeyCode,
    surface: &a2ui::core::model::surface_model::SurfaceModel,
    registry: &a2ui::tui::component_impl::ComponentRegistry,
    catalog: &Catalog,
    focus_manager: &FocusManager,
) -> Option<EventResult> {
    let input_key = map_key(code)?;
    if matches!(input_key, InputKey::Tab | InputKey::BackTab) {
        return None;
    }
    let focused_id = focus_manager.focused_id()?.to_string();
    let input_event = InputEvent::KeyPress { key: input_key };

    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();
    let comp_model = components.get(&focused_id)?;
    let tui_comp = registry.get(&comp_model.component_type)?;
    let ctx = ComponentContext::new(
        focused_id.clone(),
        surface.id.clone(),
        &data_model,
        &components,
        &catalog.functions,
        "",
        Some(focused_id.clone()),
    );
    tui_comp.handle_event(&ctx, &input_event)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = build_basic_catalog();
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
    let mut focus_manager = FocusManager::new();

    // ── 1. Surface + initial data model ──────────────────────────────────
    let create = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "dt",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "sendDataModel": true,
            "dataModel": {
                "datetime": "2026-06-13T14:30:00",
                "dateonly": "2026-06-13",
                "timeonly": "14:30:00"
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create.to_string())?)?;

    // ── 2. Component tree: three DateTimeInput modes ─────────────────────
    let update = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "dt",
            "components": [
                {"id": "root", "component": "Card", "child": "col"},
                // Each DateTimeInput fills its slot (like other inputs) and
                // carries its own label as the box title — no standalone Text
                // captions, which would leave gaps under the equal-weight split.
                {"id": "col", "component": "Column", "children": ["dt_full", "dt_date", "dt_time"],
                 "justify": "center", "align": "stretch"},
                {"id": "dt_full", "component": "DateTimeInput",
                 "label": "Date + Time  (\u{2191}/\u{2193} \u{00B1}1 day  \u{00B7}  \u{2190}/\u{2192} \u{00B1}1 hour)",
                 "value": {"path": "/datetime"}, "enableDate": true, "enableTime": true},
                {"id": "dt_date", "component": "DateTimeInput",
                 "label": "Date only  (\u{2191}/\u{2193} \u{00B1}1 day  \u{00B7}  \u{2190}/\u{2192} \u{00B1}1 month)",
                 "value": {"path": "/dateonly"}, "enableDate": true, "enableTime": false},
                {"id": "dt_time", "component": "DateTimeInput",
                 "label": "Time only  (\u{2191}/\u{2193} \u{00B1}1 min  \u{00B7}  \u{2190}/\u{2192} \u{00B1}1 hour)",
                 "value": {"path": "/timeonly"}, "enableDate": false, "enableTime": true}
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update.to_string())?)?;

    if let Some(surface) = processor.model.get_surface("dt") {
        let components = surface.components.borrow();
        focus_manager.rebuild_from_components(&components);
    }

    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // ── 3. Interactive loop ──────────────────────────────────────────────
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(7), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("dt") {
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, chunks[0], focus_manager.focused_id());

                let (dt, d, t) = {
                    let dm = surface.data_model.borrow();
                    (
                        dm.get("/datetime").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        dm.get("/dateonly").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        dm.get("/timeonly").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    )
                };
                let bar = Paragraph::new(Line::from(format!(
                    " Tab:focus   arrows:adjust   q:quit   |  datetime={dt}  date={d}  time={t}"
                )))
                .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(bar, chunks[1]);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => focus_manager.focus_next(),
                    KeyCode::BackTab => focus_manager.focus_prev(),
                    other => {
                        // Dispatch to the focused component, then apply its
                        // DataUpdate back through the processor.
                        let result = processor.model.get_surface("dt").and_then(|surface| {
                            dispatch_to_focused(other, surface, &registry, &render_catalog, &focus_manager)
                        });
                        if let Some(EventResult::DataUpdate { path, value }) = result {
                            let msg = serde_json::json!({
                                "version": "v1.0",
                                "updateDataModel": {
                                    "surfaceId": "dt", "path": path, "value": value
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

    // ── 4. Final data model ──────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    if let Some(surface) = processor.model.get_surface("dt") {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}
