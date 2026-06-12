//! # Example: Hello World
//!
//! The simplest possible a2ui program: create a surface with a Text component
//! and render it to the terminal.
//!
//! ## What it demonstrates
//! - Parsing a single A2UI JSON message
//! - Processing messages through `MessageProcessor`
//! - Rendering a surface with `SurfaceRenderer`
//!
//! ## Run
//! ```sh
//! cargo run --example 01_hello_world
//! ```

use std::io;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use a2ui::core::catalog::Catalog;
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── 1. Build the catalog and component registry ──────────────────────
    //    The "basic" catalog includes all 18 components and 14 functions.
    //    Note: MessageProcessor takes ownership of the catalog. For rendering,
    //    we create a separate placeholder catalog (functions aren't needed for
    //    this simple example). See example 05 for how to share a catalog.
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    // ── 3. Define an A2UI message as JSON ────────────────────────────────
    //    This creates a surface with a single "Hello, A2UI!" text component.
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "hello",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
        }
    });

    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "hello",
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": ["title", "subtitle"],
                    "justify": "center",
                    "align": "center"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "Hello, A2UI!",
                    "variant": "h1"
                },
                {
                    "id": "subtitle",
                    "component": "Text",
                    "text": "Press 'q' to quit",
                    "variant": "body"
                }
            ]
        }
    });

    // ── 4. Parse and process the messages ────────────────────────────────
    let msg1 = MessageProcessor::parse_message(&create_msg.to_string())?;
    let msg2 = MessageProcessor::parse_message(&update_msg.to_string())?;
    processor.process_message(msg1)?;
    processor.process_message(msg2)?;

    // ── 5. Set up the terminal and render ────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // Render loop — display the surface until the user presses 'q'.
    loop {
        let surface = processor.model.get_surface("hello").unwrap();
        terminal.draw(|frame| {
            let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                surface, &registry, &render_catalog,
            );
            renderer.render(frame, frame.area());
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    // ── 6. Restore the terminal ──────────────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    println!("Goodbye from a2ui!");
    Ok(())
}
