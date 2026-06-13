//! # Example: Interactive Form (Full Event Pipeline)
//!
//! A contact form that demonstrates the complete A2UI event handling pipeline.
//! Every interactive component (TextField, CheckBox, Slider, Button) receives
//! keyboard events via `TuiComponent::handle_event()`, and the results
//! (`DataUpdate`, `Toggle`, `Action`) are processed to update the data model
//! or dispatch actions.
//!
//! ## What it demonstrates
//! - `handle_event` on `TextField` (character input + backspace)
//! - `handle_event` on `CheckBox` (Enter/Space toggle)
//! - `handle_event` on `Slider` (Left/Right adjustment)
//! - `handle_event` on `Button` (Enter dispatches action)
//! - `EventResult::DataUpdate`, `EventResult::Toggle`, `EventResult::Action`
//! - Focus visual feedback (yellow borders on focused components)
//! - `build_basic_catalog()` for both processor and renderer
//!
//! ## Run
//! ```sh
//! cargo run --example 09_interactive_form
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

// ---------------------------------------------------------------------------
// Event dispatch helper
// ---------------------------------------------------------------------------

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

/// Dispatch a keyboard event to the focused component.
///
/// Returns `Some(EventResult)` if the component handled the event, `None` otherwise.
/// The caller is responsible for processing the result (updating data model, etc.).
fn dispatch_to_focused(
    code: KeyCode,
    surface: &a2ui::core::model::surface_model::SurfaceModel,
    registry: &a2ui::tui::component_impl::ComponentRegistry,
    catalog: &Catalog,
    focus_manager: &FocusManager,
) -> Option<EventResult> {
    let input_key = map_key(code)?;

    // Skip Tab/BackTab — those are handled by the focus manager directly.
    if matches!(input_key, InputKey::Tab | InputKey::BackTab) {
        return None;
    }

    let focused_id = focus_manager.focused_id()?.to_string();
    let input_event = InputEvent::KeyPress { key: input_key };

    // Borrow both RefCells for the duration of event handling.
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();

    // Look up the focused component model and its TUI implementation.
    let comp_model = components.get(&focused_id)?;
    let tui_comp = registry.get(&comp_model.component_type)?;

    // Build a ComponentContext for the focused component.
    let ctx = ComponentContext::new(
        focused_id.clone(),
        surface.id.clone(),
        &data_model,
        &components,
        &catalog.functions,
        "",
        Some(focused_id.clone()),
    );

    // Dispatch the event to the component.
    tui_comp.handle_event(&ctx, &input_event)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = build_basic_catalog();
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
    let mut focus_manager = FocusManager::new();

    // ── 1. Create surface ────────────────────────────────────────────────
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "contact",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "sendDataModel": true,
            "dataModel": {
                "name": "",
                "email": "",
                "subscribe": false,
                "satisfaction": 50,
                "status": ""
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 2. Define component tree ─────────────────────────────────────────
    //    Structure:
    //    root: Card
    //      form: Column
    //        title: Text "Contact Form"
    //        name_field: TextField (bound to /name)
    //        email_field: TextField (bound to /email)
    //        subscribe_cb: CheckBox (bound to /subscribe)
    //        satisfaction_slider: Slider (bound to /satisfaction)
    //        divider: Divider
    //        submit_btn: Button (dispatches "form_submitted" action)
    //        status_text: Text (bound to /status)
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "contact",
            "components": [
                {
                    "id": "root",
                    "component": "Card",
                    "child": "form"
                },
                {
                    "id": "form",
                    "component": "Column",
                    "children": [
                        "title",
                        "name_field",
                        "email_field",
                        "subscribe_cb",
                        "satisfaction_slider",
                        "divider_1",
                        "submit_label",
                        "submit_btn",
                        "status_text"
                    ],
                    "justify": "center",
                    "align": "stretch"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "Contact Form",
                    "variant": "h2"
                },
                {
                    "id": "name_field",
                    "component": "TextField",
                    "label": "Name",
                    "value": {"path": "/name"},
                    "variant": "shortText",
                    "placeholder": "Enter your name..."
                },
                {
                    "id": "email_field",
                    "component": "TextField",
                    "label": "Email",
                    "value": {"path": "/email"},
                    "variant": "shortText",
                    "placeholder": "Enter your email..."
                },
                {
                    "id": "subscribe_cb",
                    "component": "CheckBox",
                    "label": "Subscribe to newsletter",
                    "value": {"path": "/subscribe"}
                },
                {
                    "id": "satisfaction_slider",
                    "component": "Slider",
                    "label": "Satisfaction",
                    "value": {"path": "/satisfaction"},
                    "min": 0,
                    "max": 100,
                    "steps": 10
                },
                {
                    "id": "divider_1",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "submit_label",
                    "component": "Text",
                    "text": "Submit"
                },
                {
                    "id": "submit_btn",
                    "component": "Button",
                    "child": "submit_label",
                    "variant": "primary",
                    "action": {
                        "event": {
                            "name": "form_submitted",
                            "context": {
                                "name": {"path": "/name"},
                                "email": {"path": "/email"},
                                "subscribe": {"path": "/subscribe"},
                                "satisfaction": {"path": "/satisfaction"}
                            }
                        }
                    }
                },
                {
                    "id": "status_text",
                    "component": "Text",
                    "text": {"path": "/status"},
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    // ── 3. Set up focus management and terminal ──────────────────────────
    if let Some(surface) = processor.model.get_surface("contact") {
        let components = surface.components.borrow();
        focus_manager.rebuild_from_components(&components);
    }

    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // ── 4. Interactive loop ──────────────────────────────────────────────
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("contact") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                let focused = focus_manager.focused_id();
                renderer.render(frame, chunks[0], focused);

                // Build help bar showing data model state.
                let (name, email, sub, sat) = {
                    let dm = surface.data_model.borrow();
                    (
                        dm.get("/name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        dm.get("/email").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        dm.get("/subscribe").and_then(|v| v.as_bool()).unwrap_or(false),
                        dm.get("/satisfaction").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    )
                };

                let help_text = format!(
                    " Tab/Shift+Tab: navigate  |  name={}  email={}  sub={}  sat={:.0}  |  q: quit ",
                    name, email, sub, sat,
                );
                let bar = Paragraph::new(Line::from(help_text))
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
                        // Phase 1: dispatch event to focused component (immutable borrow).
                        let result = processor.model.get_surface("contact").and_then(|surface| {
                            dispatch_to_focused(
                                other,
                                surface,
                                &registry,
                                &render_catalog,
                                &focus_manager,
                            )
                        });

                        // Phase 2: process the result (mutable borrow of processor).
                        if let Some(result) = result {
                            match result {
                                EventResult::DataUpdate { path, value } => {
                                    let msg = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "contact",
                                            "path": path,
                                            "value": value,
                                        }
                                    });
                                    let _ = processor.process_message(
                                        MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                                    );
                                }
                                EventResult::Toggle { path } => {
                                    let current = processor.model.get_surface("contact")
                                        .map(|s| {
                                            let dm = s.data_model.borrow();
                                            dm.get(&path).and_then(|v| v.as_bool()).unwrap_or(false)
                                        })
                                        .unwrap_or(false);
                                    let msg = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "contact",
                                            "path": path,
                                            "value": !current,
                                        }
                                    });
                                    let _ = processor.process_message(
                                        MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                                    );
                                }
                                EventResult::Action { event_name, context, .. } => {
                                    eprintln!("[ACTION] {} {:?}", event_name, context);
                                    // Show success status in data model.
                                    let msg = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "contact",
                                            "path": "/status",
                                            "value": "Form submitted successfully! (check stderr for details)"
                                        }
                                    });
                                    let _ = processor.process_message(
                                        MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                                    );
                                }
                                EventResult::Consumed => {}
                            }
                        }
                    }
                }
            }
        }
    }

    // ── 5. Show final data model state ───────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    if let Some(surface) = processor.model.get_surface("contact") {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}
