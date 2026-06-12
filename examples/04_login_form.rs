//! # Example: Login Form
//!
//! A realistic login form built entirely from A2UI JSON messages, featuring
//! TextField inputs with data bindings, a Button with an action, and
//! validation checks.
//!
//! ## What it demonstrates
//! - `TextField` with `value: {"path": "..."}` data bindings
//! - `Button` with `action` event definitions
//! - `Card` as a form container
//! - `checks` array for field validation (required, email)
//! - Building a complete form UI from A2UI protocol messages
//!
//! ## Run
//! ```sh
//! cargo run --example 04_login_form
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
use a2ui::tui::focus_manager::FocusManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
    let mut focus_manager = FocusManager::new();

    // ── 1. Create surface with send data model enabled ───────────────────
    //    send_data_model: true means actions will include the full data model.
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "login",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "sendDataModel": true,
            "dataModel": {
                "username": "",
                "password": ""
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 2. Define the login form component tree ──────────────────────────
    //    Structure: Card > Column > [title, username_field, password_field, button]
    //
    //    Key concepts:
    //    - "value": {"path": "/username"} binds the TextField to the data model
    //    - "checks": [...] adds validation rules
    //    - "action": {"event": {...}} defines what event fires on button click
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "login",
            "components": [
                {
                    "id": "root",
                    "component": "Card",
                    "child": "form"
                },
                {
                    "id": "form",
                    "component": "Column",
                    "children": ["title", "username_field", "password_field", "divider_1", "submit_btn"],
                    "justify": "center",
                    "align": "stretch"
                },
                {
                    "id": "title",
                    "component": "Text",
                    "text": "Welcome Back",
                    "variant": "h2"
                },
                {
                    "id": "username_field",
                    "component": "TextField",
                    "label": "Username",
                    "value": {"path": "/username"},
                    "variant": "shortText",
                    "checks": [
                        {
                            "call": "required",
                            "args": {"value": {"path": "/username"}},
                            "message": "Username is required."
                        }
                    ]
                },
                {
                    "id": "password_field",
                    "component": "TextField",
                    "label": "Password",
                    "value": {"path": "/password"},
                    "variant": "obscured",
                    "checks": [
                        {
                            "call": "required",
                            "args": {"value": {"path": "/password"}},
                            "message": "Password is required."
                        },
                        {
                            "call": "length",
                            "args": {"value": {"path": "/password"}, "min": 6},
                            "message": "Password must be at least 6 characters."
                        }
                    ]
                },
                {
                    "id": "divider_1",
                    "component": "Divider",
                    "axis": "horizontal"
                },
                {
                    "id": "submit_label",
                    "component": "Text",
                    "text": "Sign In"
                },
                {
                    "id": "submit_btn",
                    "component": "Button",
                    "child": "submit_label",
                    "variant": "primary",
                    "action": {
                        "event": {
                            "name": "login_submitted",
                            "context": {
                                "user": {"path": "/username"},
                                "pass": {"path": "/password"}
                            }
                        }
                    }
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    // ── 3. Set up focus management and terminal ──────────────────────────
    if let Some(surface) = processor.model.get_surface("login") {
        let components = surface.components.borrow();
        focus_manager.rebuild_from_components(&components);
    }

    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // ── 4. Interactive loop with keyboard focus navigation ───────────────
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(2)])
                .split(area);

            if let Some(surface) = processor.model.get_surface("login") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                renderer.render(frame, chunks[0]);
            }

            let help = " Tab: next field  Shift+Tab: prev field  q: quit ";
            let bar = Paragraph::new(Line::from(help))
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => focus_manager.focus_next(),
                    KeyCode::BackTab => focus_manager.focus_prev(),
                    _ => {}
                }
            }
        }
    }

    // ── 5. Show final data model state before exiting ────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    if let Some(surface) = processor.model.get_surface("login") {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}
