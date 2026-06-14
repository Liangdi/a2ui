//! # Example: Server-Initiated Function Call (`callFunction`)
//!
//! Demonstrates how the server can invoke client-side functions via the
//! `callFunction` message, and how the client responds with
//! `functionResponse` or `error` messages.
//!
//! ## What it demonstrates
//! - Sending a `callFunction` message to execute a catalog function
//! - `wantResponse: true` → `drain_outgoing()` returns a `functionResponse`
//! - `wantResponse: false` → silent execution, no outgoing message
//! - Calling an unregistered function → `drain_outgoing()` returns an `error`
//! - Writing the `functionResponse` result back into the data model
//!
//! ## Run
//! ```sh
//! cargo run --example 06_call_function
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
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::protocol::client_to_server::ClientPayload;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    // Track last outgoing message for display
    let mut status_text = String::new();

    // ── 1. Create surface with initial data ──────────────────────────────
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "demo",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel": {
                "name": "Alice",
                "greeting": "Press 'f' to call a function"
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 2. Define component tree ────────────────────────────────────────
    //    The "greeting" Text is bound to /greeting in the data model.
    //    When we write a functionResponse value there, the UI updates.
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "demo",
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": ["title", "name_display", "divider_1", "greeting", "divider_2", "status"],
                    "justify": "center",
                    "align": "stretch"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "callFunction Demo",
                    "variant": "h1"
                },
                {
                    "id": "name_display",
                    "component": "Text",
                    "text": {"path": "/name"},
                    "variant": "h3"
                },
                {
                    "id": "divider_1",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "greeting",
                    "component": "Text",
                    "text": {"path": "/greeting"},
                    "variant": "body"
                },
                {
                    "id": "divider_2",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "status",
                    "component": "Text",
                    "text": {"path": "/status"},
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    // ── 3. Demonstrate callFunction with wantResponse ───────────────────
    //    The server asks the client to execute "formatString".
    //    With wantResponse: true, the result comes back via drain_outgoing().
    let call_msg = serde_json::json!({
        "version": "v1.0",
        "functionCallId": "call_1",
        "wantResponse": true,
        "callFunction": {
            "call": "formatString",
            "args": { "value": "Hello, ${/name}! Welcome to A2UI." }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&call_msg.to_string())?)?;

    // Drain the outgoing messages produced by callFunction
    let outgoing = processor.drain_outgoing();
    if let Some(msg) = outgoing.first() {
        match &msg.payload {
            ClientPayload::FunctionResponse(fr) => {
                let value = &fr.function_response.value;
                // Write the function result into the data model at /greeting
                let write_msg = serde_json::json!({
                    "version": "v1.0",
                    "updateDataModel": {
                        "surfaceId": "demo",
                        "path": "/greeting",
                        "value": value
                    }
                });
                processor.process_message(
                    MessageProcessor::parse_message(&write_msg.to_string())?,
                )?;
                status_text = format!(
                    "functionResponse: call={}, value={}",
                    fr.function_response.call, value
                );
            }
            ClientPayload::Error(err) => {
                status_text = format!("Error: {}", err.error.message);
            }
            _ => {}
        }
    }

    // Update status text in data model
    let status_update = serde_json::json!({
        "version": "v1.0",
        "updateDataModel": {
            "surfaceId": "demo",
            "path": "/status",
            "value": status_text
        }
    });
    processor.process_message(MessageProcessor::parse_message(&status_update.to_string())?)?;

    // ── 4. Interactive loop ─────────────────────────────────────────────
    //    f: call formatString (with wantResponse) → update greeting
    //    e: call nonexistent function → see error response
    //    n: cycle name in data model
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    let names = ["Alice", "Bob", "Charlie", "Diana"];
    let mut name_idx = 0;
    let mut call_counter = 1u32;

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("demo") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                renderer.render(frame, chunks[0], None);
            }

            let help = " f: call function  e: call invalid  n: cycle name  q: quit ";
            let bar = Paragraph::new(Line::from(help))
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('n') => {
                        name_idx = (name_idx + 1) % names.len();
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "demo",
                                "path": "/name",
                                "value": names[name_idx]
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&msg.to_string())?,
                        )?;
                    }
                    KeyCode::Char('f') => {
                        // Call formatString with data binding, wantResponse: true
                        call_counter += 1;
                        let call_id = format!("call_{}", call_counter);
                        let call_msg = serde_json::json!({
                            "version": "v1.0",
                            "functionCallId": call_id,
                            "wantResponse": true,
                            "callFunction": {
                                "call": "formatString",
                                "args": { "value": "Hello, ${/name}! The answer is 42." }
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&call_msg.to_string())?,
                        )?;

                        let outgoing = processor.drain_outgoing();
                        let new_status = if let Some(msg) = outgoing.first() {
                            match &msg.payload {
                                ClientPayload::FunctionResponse(fr) => {
                                    let value = &fr.function_response.value;
                                    // Write result to data model
                                    let write_msg = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "demo",
                                            "path": "/greeting",
                                            "value": value
                                        }
                                    });
                                    processor.process_message(
                                        MessageProcessor::parse_message(&write_msg.to_string())?,
                                    )?;
                                    format!("✓ functionResponse: {}", value)
                                }
                                ClientPayload::Error(err) => {
                                    format!("✗ Error: {}", err.error.message)
                                }
                                _ => "unknown response".to_string(),
                            }
                        } else {
                            "no response (wantResponse was false)".to_string()
                        };

                        let status_update = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "demo",
                                "path": "/status",
                                "value": new_status
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&status_update.to_string())?,
                        )?;
                    }
                    KeyCode::Char('e') => {
                        // Call a nonexistent function → should produce an error
                        call_counter += 1;
                        let call_id = format!("call_{}", call_counter);
                        let call_msg = serde_json::json!({
                            "version": "v1.0",
                            "functionCallId": call_id,
                            "wantResponse": true,
                            "callFunction": {
                                "call": "thisFunctionDoesNotExist",
                                "args": {}
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&call_msg.to_string())?,
                        )?;

                        let outgoing = processor.drain_outgoing();
                        if let Some(msg) = outgoing.first() {
                            match &msg.payload {
                                ClientPayload::Error(err) => {
                                    let status = format!("✗ Error: {} (code: {})", err.error.message, err.error.code);
                                    let status_update = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "demo",
                                            "path": "/status",
                                            "value": status
                                        }
                                    });
                                    processor.process_message(
                                        MessageProcessor::parse_message(&status_update.to_string())?,
                                    )?;
                                }
                                other => {
                                    let status = format!("Unexpected response: {:?}", other);
                                    let status_update = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "demo",
                                            "path": "/status",
                                            "value": status
                                        }
                                    });
                                    processor.process_message(
                                        MessageProcessor::parse_message(&status_update.to_string())?,
                                    )?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    // Show final state
    if let Some(surface) = processor.model.get_surface("demo") {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}
