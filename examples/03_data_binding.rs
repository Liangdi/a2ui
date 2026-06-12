//! # Example: Reactive Data Binding
//!
//! Demonstrates how A2UI's data model drives dynamic text content through
//! JSON Pointer bindings. Updating the data model automatically changes
//! what the UI displays.
//!
//! ## What it demonstrates
//! - Creating a surface with an initial `dataModel`
//! - Dynamic text via `{"path": "/some/field"}` bindings
//! - Using `updateDataModel` to reactively update the UI
//!
//! ## Run
//! ```sh
//! cargo run --example 03_data_binding
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

    // ── 1. Create a surface with an initial data model ───────────────────
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "profile",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel": {
                "name": "Alice",
                "role": "Engineer",
                "count": 0
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 2. Define components that bind to the data model ─────────────────
    //    The `text` field uses a DynamicString binding: `{"path": "/name"}`
    //    resolves to the current value at that JSON Pointer in the data model.
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "profile",
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": ["greeting", "role_line", "counter", "help"],
                    "justify": "center",
                    "align": "center"
                },
                {
                    "id": "greeting",
                    "component": "Text",
                    "text": {"path": "/name"},
                    "variant": "h1"
                },
                {
                    "id": "role_line",
                    "component": "Text",
                    "text": {"path": "/role"},
                    "variant": "h3"
                },
                {
                    "id": "counter",
                    "component": "Text",
                    "text": {"path": "/count"},
                    "variant": "body"
                },
                {
                    "id": "help",
                    "component": "Text",
                    "text": "n: change name  r: change role  c: increment counter  q: quit",
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    // ── 3. Interactive loop: update data model and watch UI react ────────
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    let names = ["Alice", "Bob", "Charlie", "Diana"];
    let roles = ["Engineer", "Designer", "Manager", "Scientist"];
    let mut name_idx = 0;
    let mut role_idx = 0;
    let mut count = 0u32;

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("profile") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                renderer.render(frame, chunks[0]);
            }

            let data = format!(
                " Data: name={:?}  role={:?}  count={} ",
                names[name_idx], roles[role_idx], count
            );
            let bar = Paragraph::new(Line::from(data))
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('n') => {
                        // Cycle through names and update the data model.
                        name_idx = (name_idx + 1) % names.len();
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "profile",
                                "path": "/name",
                                "value": names[name_idx]
                            }
                        });
                        let _ = processor.process_message(
                            MessageProcessor::parse_message(&msg.to_string()).unwrap()
                        );
                    }
                    KeyCode::Char('r') => {
                        // Cycle through roles.
                        role_idx = (role_idx + 1) % roles.len();
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "profile",
                                "path": "/role",
                                "value": roles[role_idx]
                            }
                        });
                        let _ = processor.process_message(
                            MessageProcessor::parse_message(&msg.to_string()).unwrap()
                        );
                    }
                    KeyCode::Char('c') => {
                        // Increment counter.
                        count += 1;
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "profile",
                                "path": "/count",
                                "value": count
                            }
                        });
                        let _ = processor.process_message(
                            MessageProcessor::parse_message(&msg.to_string()).unwrap()
                        );
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
