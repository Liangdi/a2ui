//! # Example: Template List (Dynamic Data-Bound Lists)
//!
//! Demonstrates template rendering with dynamic data-bound lists. A `List`
//! component uses `ChildList::Template` to iterate over a data array and
//! render the same component for each item, with per-item data scoping.
//!
//! ## What it demonstrates
//! - `List` with `ChildList::Template` children
//! - Template iteration with per-item data scoping via `base_path`
//! - `Row` layout within a template component
//! - `formatCurrency` function for price display
//! - `Justify::SpaceBetween` layout for spreading items
//! - Data model updates that re-render the template automatically
//!
//! ## Run
//! ```sh
//! cargo run --example 10_template_list
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

use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui::tui::focus_manager::FocusManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = build_basic_catalog();
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
    let mut focus_manager = FocusManager::new();

    // ── 1. Create surface with shopping list data ────────────────────────
    //    The `items` array contains objects with `name` and `price` fields.
    //    The template iterates over this array, scoping each item via base_path.
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "shopping",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel": {
                "items": [
                    {"name": "Apple",      "price": 1.20},
                    {"name": "Banana",     "price": 0.80},
                    {"name": "Cherry",     "price": 3.50},
                    {"name": "Date",       "price": 2.10},
                    {"name": "Elderberry", "price": 4.75}
                ],
                "total": 0.0
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 2. Define component tree ─────────────────────────────────────────
    //    Structure:
    //    root: Column
    //      title: Text "Shopping List" (h2)
    //      subtitle: Text showing total via formatCurrency
    //      divider: Divider
    //      item_list: List (vertical) with Template children
    //        template iterates over /items array
    //          item_template: Row (horizontal, SpaceBetween)
    //            item_name: Text with path "name" (resolves per-item)
    //            item_price: Text with formatCurrency function call
    //      divider2: Divider
    //      footer: Text caption
    //
    //    Key: The List's children is a Template:
    //      { "componentId": "item_template", "path": "/items" }
    //    For each item i in /items, the renderer creates a nested DataContext
    //    with base_path = "/items/0", "/items/1", etc.
    //    So "name" resolves to "/items/0/name", "/items/1/name", etc.
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "shopping",
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": [
                        "title",
                        "subtitle",
                        "divider_1",
                        "item_list",
                        "divider_2",
                        "footer"
                    ],
                    "justify": "start",
                    "align": "stretch"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "Shopping List",
                    "variant": "h2"
                },
                {
                    "id": "subtitle",
                    "component": "Text",
                    "text": {
                        "call": "formatString",
                        "args": {
                            "value": "5 items in your basket"
                        }
                    },
                    "variant": "body"
                },
                {
                    "id": "divider_1",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "item_list",
                    "component": "List",
                    "children": {
                        "componentId": "item_template",
                        "path": "/items"
                    }
                },
                {
                    "id": "item_template",
                    "component": "Row",
                    "children": ["item_name", "item_price"],
                    "justify": "spaceBetween",
                    "align": "center"
                },
                {
                    "id": "item_name",
                    "component": "Text",
                    "text": {"path": "name"},
                    "variant": "body"
                },
                {
                    "id": "item_price",
                    "component": "Text",
                    "text": {
                        "call": "formatCurrency",
                        "args": {
                            "value": {"path": "price"},
                            "currency": "USD"
                        }
                    },
                    "variant": "body"
                },
                {
                    "id": "divider_2",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "footer",
                    "component": "Text",
                    "text": "r: reset items  s: shuffle prices  q: quit",
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    // ── 3. Set up focus management and terminal ──────────────────────────
    if let Some(surface) = processor.model.get_surface("shopping") {
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

            if let Some(surface) = processor.model.get_surface("shopping") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                renderer.render(frame, chunks[0], None);
            }

            let help = " r: reset  s: shuffle prices  q: quit ";
            let bar = Paragraph::new(Line::from(help))
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => {
                        // Reset items to original values.
                        let msg = serde_json::json!({
                            "version": "v1.0",
                            "updateDataModel": {
                                "surfaceId": "shopping",
                                "path": "/items",
                                "value": [
                                    {"name": "Apple",      "price": 1.20},
                                    {"name": "Banana",     "price": 0.80},
                                    {"name": "Cherry",     "price": 3.50},
                                    {"name": "Date",       "price": 2.10},
                                    {"name": "Elderberry", "price": 4.75}
                                ]
                            }
                        });
                        let _ = processor.process_message(
                            MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                        );
                    }
                    KeyCode::Char('s') => {
                        // Shuffle prices: multiply by a random-ish factor.
                        let surface = processor.model.get_surface("shopping").unwrap();
                        let dm = surface.data_model.borrow();
                        let items = dm.get("/items").and_then(|v| v.as_array()).cloned();
                        drop(dm);

                        if let Some(items) = items {
                            let new_items: Vec<serde_json::Value> = items
                                .iter()
                                .enumerate()
                                .map(|(i, item)| {
                                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                    let price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(1.0);
                                    // Pseudo-random price shift using index.
                                    let new_price = ((price * (1.0 + (i as f64 * 0.37).sin()) * 100.0).round() / 100.0).max(0.10);
                                    serde_json::json!({"name": name, "price": new_price})
                                })
                                .collect();

                            let msg = serde_json::json!({
                                "version": "v1.0",
                                "updateDataModel": {
                                    "surfaceId": "shopping",
                                    "path": "/items",
                                    "value": new_items
                                }
                            });
                            let _ = processor.process_message(
                                MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ── 5. Cleanup ───────────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    if let Some(surface) = processor.model.get_surface("shopping") {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}
