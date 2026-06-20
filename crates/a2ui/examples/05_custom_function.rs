//! # Example: Custom Catalog Function
//!
//! Demonstrates how to create a custom catalog with your own function
//! implementation. This example adds a `greet` function that generates
//! a personalized greeting from the data model.
//!
//! ## What it demonstrates
//! - Implementing the `FunctionImplementation` trait
//! - Building a custom `Catalog` with a custom function
//! - Using the function in a DynamicString binding
//! - How functions are resolved through the DataContext during rendering
//!
//! ## Run
//! ```sh
//! cargo run --example 05_custom_function
//! ```

use std::collections::HashMap;
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

use a2ui::core::catalog::function_api::{FunctionImplementation, ReturnType};
use a2ui::core::error::A2uiError;
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::model::data_context::DataContext;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

// ─────────────────────────────────────────────────────────────────────────────
// Custom function: "greet"
// ─────────────────────────────────────────────────────────────────────────────
// This function takes a "name" argument and an optional "emoji" argument,
// and returns a greeting string like "👋 Hello, Alice!"
//
// In A2UI JSON it would be used like:
//   { "call": "greet", "args": { "name": {"path": "/user"}, "emoji": "🎉" } }

struct GreetFunction;

impl FunctionImplementation for GreetFunction {
    fn name(&self) -> &'static str {
        "greet"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, serde_json::Value>,
        _context: &DataContext,
    ) -> Result<serde_json::Value, A2uiError> {
        // Extract the "name" argument (already resolved by DataContext).
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");

        // Extract optional "emoji" argument.
        let emoji = args.get("emoji").and_then(|v| v.as_str()).unwrap_or("👋");

        let greeting = format!("{} Hello, {}!", emoji, name);
        Ok(serde_json::Value::String(greeting))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Another custom function: "upper"
// ─────────────────────────────────────────────────────────────────────────────
// A simple function that uppercases a string value.

struct UpperFunction;

impl FunctionImplementation for UpperFunction {
    fn name(&self) -> &'static str {
        "upper"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, serde_json::Value>,
        _context: &DataContext,
    ) -> Result<serde_json::Value, A2uiError> {
        let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("");
        Ok(serde_json::Value::String(value.to_uppercase()))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();

    // ── 1. Build a catalog with custom functions ─────────────────────────
    //    Start from the basic catalog (for all standard components), then
    //    add our custom "greet" and "upper" functions.
    let catalog = build_basic_catalog()
        .with_function(Box::new(GreetFunction))
        .with_function(Box::new(UpperFunction));

    // We need the catalog for both the processor (takes ownership) and
    // the renderer (borrows). Build a second one for rendering.
    let render_catalog = build_basic_catalog()
        .with_function(Box::new(GreetFunction))
        .with_function(Box::new(UpperFunction));

    let mut processor = MessageProcessor::new(vec![catalog]);

    // ── 2. Create a surface with data for the function to consume ────────
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "custom",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel": {
                "user": "A2UI Developer"
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 3. Use the custom function in a DynamicString ────────────────────
    //    The "greeting" text uses a function call that resolves /user
    //    from the data model and passes it to our "greet" function.
    //
    //    JSON structure for a function call in a DynamicString:
    //      { "call": "greet", "args": { "name": {"path": "/user"} } }
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "custom",
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": ["title", "greeting", "divider", "upper_demo", "help"],
                    "justify": "center",
                    "align": "center"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "Custom Functions",
                    "variant": "h1"
                },
                {
                    "id": "greeting",
                    "component": "Text",
                    "text": {
                        "call": "greet",
                        "args": {
                            "name": {"path": "/user"},
                            "emoji": "🚀"
                        }
                    },
                    "variant": "h2"
                },
                {
                    "id": "divider",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "upper_demo",
                    "component": "Text",
                    "text": {
                        "call": "upper",
                        "args": {
                            "value": {"path": "/user"}
                        }
                    },
                    "variant": "body"
                },
                {
                    "id": "help",
                    "component": "Text",
                    "text": "n: cycle name  q: quit",
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    // ── 4. Interactive loop ──────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    let names = ["A2UI Developer", "Rustacean", "Terminal Hacker", "Claude"];
    let mut idx = 0;

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("custom") {
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, chunks[0], None);
            }

            let bar = Paragraph::new(Line::from(format!(
                " Data: user={:?}  |  n: cycle name  q: quit ",
                names[idx]
            )))
            .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('n') => {
                        idx = (idx + 1) % names.len();
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "custom",
                                "path": "/user",
                                "value": names[idx]
                            }
                        });
                        let _ = processor.process_message(
                            MessageProcessor::parse_message(&msg.to_string()).unwrap(),
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
