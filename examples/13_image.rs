//! # Example: Real Image Rendering
//!
//! Renders an actual image inside an A2UI `Image` component using
//! `ratatui-image`. The `Image` component loads the image from a LOCAL file
//! path (it does not fetch HTTP URLs) and renders it via the best native
//! graphics protocol the terminal supports — probing once and **degrading
//! automatically**: **kitty → iTerm2 → Sixel → Halfblocks**. Halfblocks (the
//! last resort) works in *every* terminal via colored half-block glyphs. The
//! status bar shows which protocol actually won in your terminal.
//!
//! This example drives the real A2UI rendering path: it builds a small
//! component tree via A2UI JSON messages and renders it through
//! `SurfaceRenderer`, exactly like any other component. The tree is a
//! `Card` (rounded-border container) wrapping a `Column` that holds a
//! styled `Text` title and the `Image`. The `ImageComponent` performs the
//! real rendering; if decoding fails it falls back to the `[🖼 ...]` text
//! placeholder.
//!
//! Defaults to `examples/assets/bad-apple.png`; pass a path as the first CLI
//! argument to use your own image.
//!
//! ## Run
//! ```sh
//! cargo run --example 13_image
//! # or with your own image:
//! cargo run --example 13_image -- /path/to/image.png
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
    // ── 1. Resolve the image path: CLI arg, else the bundled sample asset ─
    let default_path = format!("{}/examples/assets/bad-apple.png", env!("CARGO_MANIFEST_DIR"));
    let image_path = std::env::args().nth(1).unwrap_or(default_path);
    if !std::path::Path::new(&image_path).is_file() {
        return Err(format!("image file not found: {image_path}").into());
    }

    // ── 2. Build an A2UI surface whose root is a single Image component ───
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    let create = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "img",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
        }
    });
    // Root is a `Card` (rounded-border container) whose single child is a
    // `Column` holding a styled title (`Text`, `h2` variant) and the `Image`.
    // Children are always referenced by ID — never defined inline (per spec).
    // `url` is a local file path; `description` is shown if rendering falls
    // back to the placeholder.
    let update = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "img",
            "components": [
                {
                    "id": "root",
                    "component": "Card",
                    "child": "body"
                },
                {
                    "id": "body",
                    "component": "Column",
                    "children": ["title", "image"]
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "A2UI Image Demo",
                    "variant": "h2"
                },
                {
                    "id": "image",
                    "component": "Image",
                    "url": image_path,
                    "description": "A2UI Image Demo"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create.to_string())?)?;
    processor.process_message(MessageProcessor::parse_message(&update.to_string())?)?;

    // ── 3. Terminal: render the surface in a loop (re-rendering an image is ─
    //    idempotent, so a redraw loop is safe).
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
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("img") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface,
                    &registry,
                    &render_catalog,
                );
                renderer.render(frame, chunks[0], None);
            }

            // Show the protocol the renderer actually settled on after probing
            // the terminal (kitty / iTerm2 / Sixel / Halfblocks), so the
            // degradation chain is visible rather than assumed.
            let proto = a2ui::tui::components::image::detected_protocol();
            let bar = Paragraph::new(Line::from(format!(
                " ratatui-image · protocol: {proto} · q: quit",
            )))
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

    // ── 4. Restore the terminal ──────────────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    println!("Goodbye! (image rendered via ratatui-image)");
    Ok(())
}
