//! # Example: JSONL Stream Processing
//!
//! Demonstrates how to parse a newline-delimited JSON (JSONL) stream of A2UI
//! messages and progressively step through them.
//!
//! ## What it demonstrates
//! - `MessageProcessor::parse_jsonl()` for batch parsing
//! - Progressive message processing (stepper pattern)
//! - Surface lifecycle: create → update → delete
//!
//! ## Run
//! ```sh
//! cargo run --example 02_jsonl_stream
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
    widgets::{Block, Borders, Paragraph},
};

use a2ui::core::catalog::Catalog;
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::protocol::server_to_client::A2uiMessage;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");

    // ── 1. Define a JSONL stream of A2UI messages ────────────────────────
    //    Each line is a complete A2UI JSON message. This simulates what an
    //    AI agent would send over a stream.
    let jsonl = r#"
{"version":"v1.0","createSurface":{"surfaceId":"demo","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"}}
{"version":"v1.0","updateComponents":{"surfaceId":"demo","components":[{"id":"root","component":"Column","children":["title","desc","divider","info_card"],"justify":"start","align":"stretch"},{"id":"title","component":"Text","text":"JSONL Stream Demo","variant":"h1"},{"id":"desc","component":"Text","text":"This UI was built from 4 separate messages in a JSONL stream.","variant":"body"}]}}
{"version":"v1.0","updateComponents":{"surfaceId":"demo","components":[{"id":"divider","component":"Divider","axis":"horizontal"},{"id":"info_card","component":"Card","child":"card_content"},{"id":"card_content","component":"Column","children":["card_title","card_body"],"justify":"start","align":"stretch"},{"id":"card_title","component":"Text","text":"How it works","variant":"h3"},{"id":"card_body","component":"Text","text":"Press 'n' to process the next message. Press 'a' to process all remaining. Press 'q' to quit.","variant":"body"}]}}
{"version":"v1.0","updateDataModel":{"surfaceId":"demo","path":"/status","value":"complete"}}
"#;

    // ── 2. Parse the entire JSONL stream at once ─────────────────────────
    let parsed = MessageProcessor::parse_jsonl(jsonl);
    let messages: Vec<A2uiMessage> = parsed.into_iter().collect::<Result<Vec<_>, _>>()?;

    let total = messages.len();

    // ── 3. Set up terminal ───────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // ── 4. Interactive stepper loop ──────────────────────────────────────
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
    let mut processed = 0;

    loop {
        terminal.draw(|frame| {
            let area = frame.area();

            // Split into surface area (top) and status bar (bottom).
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            // Render the surface if it exists.
            if let Some(surface) = processor.model.get_surface("demo") {
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, chunks[0], None);
            } else if processed == 0 {
                let waiting =
                    Paragraph::new("No surface yet. Press 'n' to process the first message.")
                        .block(Block::default().borders(Borders::ALL).title(" Waiting "));
                frame.render_widget(waiting, chunks[0]);
            }

            // Status bar: show progress.
            let status = format!(
                " Messages: {}/{}  |  n: next  a: all  r: reset  q: quit ",
                processed, total
            );
            let status_bar =
                Paragraph::new(Line::from(status)).style(Style::default().fg(Color::Cyan));
            frame.render_widget(status_bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('n') => {
                        // Process the next message.
                        if processed < total {
                            let _ = processor.process_message(messages[processed].clone());
                            processed += 1;
                        }
                    }
                    KeyCode::Char('a') => {
                        // Process all remaining messages.
                        while processed < total {
                            let _ = processor.process_message(messages[processed].clone());
                            processed += 1;
                        }
                    }
                    KeyCode::Char('r') => {
                        // Reset: recreate processor and replay from scratch.
                        processor = MessageProcessor::new(vec![build_basic_catalog()]);
                        processed = 0;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    println!("Processed {}/{} messages.", processed, total);
    Ok(())
}
