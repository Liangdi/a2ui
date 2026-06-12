//! # Debug: Card + Column minimal test
//!
//! ```sh
//! cargo run --example 07c_card_minimal
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

    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "test",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel": { "greeting": "Hello from Card!" }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "test",
            "components": [
                {
                    "id": "root",
                    "component": "Card",
                    "child": "inner"
                },
                {
                    "id": "inner",
                    "component": "Column",
                    "children": ["title", "message"],
                    "justify": "center",
                    "align": "stretch"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "Card Test",
                    "variant": "h1"
                },
                {
                    "id": "message",
                    "component": "Text",
                    "text": {"path": "/greeting"},
                    "variant": "body"
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

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("test") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                renderer.render(frame, chunks[0]);
            }

            let bar = Paragraph::new(Line::from(" q: quit"))
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}
