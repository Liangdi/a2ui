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

use a2ui::core::event::{EventResult, InputEvent};
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui::tui::focus_manager::FocusManager;
use a2ui::tui::interaction;

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
                        let key = interaction::map_key_code(other);
                        let result = key.and_then(|k| {
                            let event = InputEvent::KeyPress { key: k };
                            interaction::dispatch_to_focused(
                                &processor,
                                &registry,
                                &render_catalog,
                                &focus_manager,
                                &event,
                            )
                        });

                        // Phase 2: process the result (mutable borrow of processor).
                        if let Some(result) = result {
                            match result {
                                // This example has custom Action side effects
                                // (log + status banner) on top of the shared
                                // apply pipeline, so handle Action here and
                                // route everything else through `apply_event_result`.
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
                                other => {
                                    interaction::apply_event_result(&mut processor, other);
                                }
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
