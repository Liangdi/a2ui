//! # Example: Action Response — Debug Version (Card + many children)
//!
//! This is the original version with Card container and 9 children.
//! Used to debug which component causes the rendering issue.
//!
//! ## Run
//! ```sh
//! cargo run --example 07b_action_response_debug
//! ```

use std::io;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
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

    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "search",
            "components": [
                {
                    "id": "root",
                    "component": "Card",
                    "child": "container"
                },
                {
                    "id": "container",
                    "component": "Column",
                    "children": ["title", "query_label", "query_display", "divider_1", "result_label", "result_display", "divider_2", "status_display"],
                    "justify": "start",
                    "align": "stretch"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "actionResponse Demo",
                    "variant": "h1"
                },
                {
                    "id": "query_label",
                    "component": "Text",
                    "text": "Search query:",
                    "variant": "caption"
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
                    "id": "result_label",
                    "component": "Text",
                    "text": "Results:",
                    "variant": "caption"
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
                    "id": "status_display",
                    "component": "Text",
                    "text": {"path": "/status"},
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    let queries = ["app", "rust", "terminal", "a2ui"];
    let mut query_idx = 0;
    let mut action_counter = 0u32;

    let results: std::collections::HashMap<&str, Vec<&str>> = {
        let mut m = std::collections::HashMap::new();
        m.insert("app", vec!["apple", "application", "approved"]);
        m.insert("rust", vec!["rustacean", "rustic", "rustproof"]);
        m.insert("terminal", vec!["terminal", "terminate", "terminology"]);
        m.insert(
            "a2ui",
            vec!["a2ui protocol", "a2ui catalog", "a2ui surface"],
        );
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
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, chunks[0], None);
            }

            let help = " s: search  e: error response  n: change query  q: quit ";
            let bar = Paragraph::new(Line::from(help)).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('n') => {
                        query_idx = (query_idx + 1) % queries.len();
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "search",
                                "path": "/query",
                                "value": queries[query_idx]
                            }
                        });
                        processor
                            .process_message(MessageProcessor::parse_message(&msg.to_string())?)?;
                    }
                    KeyCode::Char('s') => {
                        action_counter += 1;
                        let action_id = format!("search_{}", action_counter);

                        processor.register_action(
                            "search",
                            &action_id,
                            Some("/searchResult".to_string()),
                        )?;

                        let suggestions = results
                            .get(queries[query_idx])
                            .map(|v| {
                                serde_json::Value::Array(
                                    v.iter()
                                        .map(|s| serde_json::Value::String(s.to_string()))
                                        .collect(),
                                )
                            })
                            .unwrap_or(serde_json::json!([]));

                        let response_msg = serde_json::json!({
                            "version": "v1.0",
                            "actionId": action_id,
                            "actionResponse": {
                                "value": suggestions
                            }
                        });
                        processor.process_message(MessageProcessor::parse_message(
                            &response_msg.to_string(),
                        )?)?;

                        let status_msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "search",
                                "path": "/status",
                                "value": format!("✓ action '{}' done", action_id)
                            }
                        });
                        processor.process_message(MessageProcessor::parse_message(
                            &status_msg.to_string(),
                        )?)?;
                    }
                    KeyCode::Char('e') => {
                        action_counter += 1;
                        let action_id = format!("search_{}", action_counter);

                        processor.register_action(
                            "search",
                            &action_id,
                            Some("/searchResult".to_string()),
                        )?;

                        let response_msg = serde_json::json!({
                            "version": "v1.0",
                            "actionId": action_id,
                            "actionResponse": {
                                "error": {
                                    "code": "SEARCH_FAILED",
                                    "message": "Server unavailable"
                                }
                            }
                        });
                        processor.process_message(MessageProcessor::parse_message(
                            &response_msg.to_string(),
                        )?)?;

                        let status_msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "search",
                                "path": "/status",
                                "value": format!("✗ action '{}' failed", action_id)
                            }
                        });
                        processor.process_message(MessageProcessor::parse_message(
                            &status_msg.to_string(),
                        )?)?;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}
