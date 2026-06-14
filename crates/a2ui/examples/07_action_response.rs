//! # Example: Action Response (`actionResponse`)
//!
//! Demonstrates how the server responds to client-initiated actions using
//! `actionResponse` messages. When a client sends an action with
//! `wantResponse: true`, the server can return a value that gets written
//! to the data model via the `responsePath` mechanism.
//!
//! ## What it demonstrates
//! - Registering a pending action with `register_action(surface_id, action_id, response_path)`
//! - Processing an `actionResponse` message from the server
//! - The `responsePath` automatically writes the response value into the data model
//! - Handling both success (`value`) and error responses
//! - Reactive UI updates triggered by the response data
//!
//! ## Run
//! ```sh
//! cargo run --example 07_action_response
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
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    // ── 1. Create surface with initial data ──────────────────────────────
    //    The UI shows a "query" and a "result" area bound to the data model.
    //    When we simulate a search:
    //      1. register_action("search", action_id, Some("/searchResult"))
    //      2. Send actionResponse with value → auto-writes to /searchResult
    //      3. UI reactively updates
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "search",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel": {
                "query": "app",
                "searchResult": "No results yet. Press 's' to search.",
                "status": "idle"
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 2. Define component tree ────────────────────────────────────────
    //    Mirrors the structure of 06_call_function for consistency.
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "search",
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": ["title", "query_display", "divider_1", "result_display", "divider_2", "status"],
                    "justify": "center",
                    "align": "stretch"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "actionResponse Demo",
                    "variant": "h1"
                },
                {
                    "id": "query_display",
                    "component": "Text",
                    "text": {"path": "/query"},
                    "variant": "h3"
                },
                {
                    "id": "divider_1",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "result_display",
                    "component": "Text",
                    "text": {"path": "/searchResult"},
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

    // ── 3. Interactive loop ─────────────────────────────────────────────
    //    s: Simulate a successful search
    //       1. register_action with responsePath = "/searchResult"
    //       2. Send actionResponse with value → auto-writes to data model
    //
    //    e: Simulate an error response
    //       1. register_action with responsePath = "/searchResult"
    //       2. Send actionResponse with error → data model unchanged
    //
    //    n: Cycle the query text
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    let queries = ["app", "rust", "terminal", "a2ui"];
    let mut query_idx = 0;
    let mut action_counter = 0u32;

    // Simulated search results per query
    let results: std::collections::HashMap<&str, Vec<&str>> = {
        let mut m = std::collections::HashMap::new();
        m.insert("app", vec!["apple", "application", "approved"]);
        m.insert("rust", vec!["rustacean", "rustic", "rustproof"]);
        m.insert("terminal", vec!["terminal", "terminate", "terminology"]);
        m.insert("a2ui", vec!["a2ui protocol", "a2ui catalog", "a2ui surface"]);
        m
    };

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("search") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                renderer.render(frame, chunks[0], None);
            }

            let help = " s: search  e: error response  n: change query  q: quit ";
            let bar = Paragraph::new(Line::from(help))
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,

                    KeyCode::Char('n') => {
                        // Cycle the query text
                        query_idx = (query_idx + 1) % queries.len();
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "search",
                                "path": "/query",
                                "value": queries[query_idx]
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&msg.to_string())?,
                        )?;
                    }

                    KeyCode::Char('s') => {
                        // ── Simulate a successful action response ──────
                        //
                        // In a real app the flow would be:
                        //   1. User triggers an action (e.g. button click)
                        //   2. Client sends `action` message to server with wantResponse: true
                        //   3. Server processes and replies with `actionResponse`
                        //
                        // Here we simulate steps 1-3 inside the client:

                        action_counter += 1;
                        let action_id = format!("search_{}", action_counter);

                        // Step 1: Set status to "searching..."
                        let status_msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "search",
                                "path": "/status",
                                "value": format!("⏳ Searching for '{}'...", queries[query_idx])
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&status_msg.to_string())?,
                        )?;

                        // Step 2: Register the pending action with responsePath
                        //   This tells the processor where to store the server's response.
                        processor.register_action(
                            "search",
                            &action_id,
                            Some("/searchResult".to_string()),
                        )?;

                        // Step 3: Simulate server's actionResponse
                        //   The responsePath "/searchResult" will be auto-written.
                        let suggestions = results
                            .get(queries[query_idx])
                            .map(|v| serde_json::Value::Array(
                                v.iter().map(|s| serde_json::Value::String(s.to_string())).collect()
                            ))
                            .unwrap_or(serde_json::json!([]));

                        let response_msg = serde_json::json!({
                            "version": "v1.0",
                            "actionId": action_id,
                            "actionResponse": {
                                "value": suggestions
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&response_msg.to_string())?,
                        )?;

                        // Step 4: Update status to show success
                        let status_msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "search",
                                "path": "/status",
                                "value": format!("✓ action '{}' completed — responsePath updated /searchResult", action_id)
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&status_msg.to_string())?,
                        )?;
                    }

                    KeyCode::Char('e') => {
                        // ── Simulate an error action response ──────────
                        //   The data model should NOT change when an error
                        //   response arrives (only "value" triggers a write).

                        action_counter += 1;
                        let action_id = format!("search_{}", action_counter);

                        processor.register_action(
                            "search",
                            &action_id,
                            Some("/searchResult".to_string()),
                        )?;

                        // Simulate server error response
                        let response_msg = serde_json::json!({
                            "version": "v1.0",
                            "actionId": action_id,
                            "actionResponse": {
                                "error": {
                                    "code": "SEARCH_FAILED",
                                    "message": "Server is temporarily unavailable"
                                }
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&response_msg.to_string())?,
                        )?;

                        // Update status to show the error
                        let status_msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "search",
                                "path": "/status",
                                "value": format!("✗ action '{}' failed — SEARCH_FAILED", action_id)
                            }
                        });
                        processor.process_message(
                            MessageProcessor::parse_message(&status_msg.to_string())?,
                        )?;
                    }

                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    // Show final state
    if let Some(surface) = processor.model.get_surface("search") {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}
