//! # Example: Tabs, Modal, and Date Formatting
//!
//! Demonstrates Tabs state switching, Modal show/hide content, and the
//! `formatDate` function with 12-hour format and AM/PM markers.
//!
//! ## What it demonstrates
//! - `Tabs` component with `activeTab` bound to `/activeTab` in the data model
//! - Left/Right arrow keys switch tabs via `handle_event`
//! - `Modal` component toggling between `trigger` and `content` children
//!   based on `isOpen` bound to `/showHelp`
//! - `formatDate` function with `h:mm a` pattern (12-hour + AM/PM)
//! - `formatDate` function with `MMM dd, yyyy` pattern
//! - Button triggering a data model update to show/hide the modal
//!
//! ## Run
//! ```sh
//! cargo run --example 11_tabs_and_modals
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
use a2ui::core::event::{EventResult, InputEvent, InputKey};
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::model::component_context::ComponentContext;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui::tui::focus_manager::FocusManager;

// ---------------------------------------------------------------------------
// Event dispatch helper (same pattern as 09_interactive_form)
// ---------------------------------------------------------------------------

fn map_key(code: KeyCode) -> Option<InputKey> {
    match code {
        KeyCode::Enter => Some(InputKey::Enter),
        KeyCode::Tab => Some(InputKey::Tab),
        KeyCode::BackTab => Some(InputKey::BackTab),
        KeyCode::Up => Some(InputKey::Up),
        KeyCode::Down => Some(InputKey::Down),
        KeyCode::Left => Some(InputKey::Left),
        KeyCode::Right => Some(InputKey::Right),
        KeyCode::Backspace => Some(InputKey::Backspace),
        KeyCode::Delete => Some(InputKey::Delete),
        KeyCode::Esc => Some(InputKey::Escape),
        KeyCode::Char(' ') => Some(InputKey::Space),
        KeyCode::Char(c) => Some(InputKey::Char(c)),
        _ => None,
    }
}

/// Dispatch a keyboard event to the focused component.
/// Returns `Some(EventResult)` if the component handled the event.
fn dispatch_to_focused(
    code: KeyCode,
    surface_id: &str,
    surface: &a2ui::core::model::surface_model::SurfaceModel,
    registry: &a2ui::tui::component_impl::ComponentRegistry,
    catalog: &Catalog,
    focus_manager: &FocusManager,
) -> Option<EventResult> {
    let input_key = map_key(code)?;

    if matches!(input_key, InputKey::Tab | InputKey::BackTab) {
        return None;
    }

    let focused_id = focus_manager.focused_id()?.to_string();
    let input_event = InputEvent::KeyPress { key: input_key };

    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();

    let comp_model = components.get(&focused_id)?;
    let tui_comp = registry.get(&comp_model.component_type)?;

    let ctx = ComponentContext::new(
        focused_id.clone(),
        surface_id.to_string(),
        &data_model,
        &components,
        &catalog.functions,
        "",
        Some(focused_id.clone()),
    );

    tui_comp.handle_event(&ctx, &input_event)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = build_basic_catalog();
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
    let mut focus_manager = FocusManager::new();

    // ── 1. Create surface ────────────────────────────────────────────────
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "tabs_demo",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "sendDataModel": true,
            "dataModel": {
                "activeTab": 0,
                "showHelp": false,
                "currentDate": "2026-06-13T14:30:00",
                "username": "A2UI User",
                "email": "user@example.com",
                "notifications": true,
                "theme": "dark",
                "version": "0.2.0"
            }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string())?)?;

    // ── 2. Define component tree ─────────────────────────────────────────
    //    Structure:
    //    root: Column
    //      header_row: Row (title + clock)
    //        title_text: Text "Tabs & Modals Demo"
    //        clock_text: Text (formatDate with h:mm a pattern)
    //      main_tabs: Tabs (3 tabs: Settings, Profile, About)
    //        settings_tab: Column (settings content)
    //        profile_tab: Column (profile content)
    //        about_tab: Column (about content)
    //      divider: Divider
    //      help_modal: Modal (trigger=help_button, content=help_panel)
    //        help_btn_label: Text "Help"
    //        help_button: Button (action: toggle_help)
    //        help_panel: Card > Column (help text)
    //      tab_hint: Text caption
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "tabs_demo",
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": [
                        "header_row",
                        "main_tabs",
                        "divider_1",
                        "help_modal",
                        "tab_hint"
                    ],
                    "justify": "start",
                    "align": "stretch"
                },
                {
                    "id": "header_row",
                    "component": "Row",
                    "children": ["title_text", "spacer_text", "clock_text"],
                    "justify": "spaceBetween",
                    "align": "center"
                },
                {
                    "id": "title_text",
                    "component": "Text",
                    "text": "Tabs & Modals Demo",
                    "variant": "h2"
                },
                {
                    "id": "spacer_text",
                    "component": "Text",
                    "text": "",
                    "variant": "body"
                },
                {
                    "id": "clock_text",
                    "component": "Text",
                    "text": {
                        "call": "formatDate",
                        "args": {
                            "value": {"path": "/currentDate"},
                            "format": "h:mm a"
                        }
                    },
                    "variant": "body"
                },

                // ── Tabs component ───────────────────────────────────
                {
                    "id": "main_tabs",
                    "component": "Tabs",
                    "activeTab": {"path": "/activeTab"},
                    "tabs": [
                        {"title": "Settings", "child": "settings_tab"},
                        {"title": "Profile",  "child": "profile_tab"},
                        {"title": "About",    "child": "about_tab"}
                    ]
                },

                // ── Settings tab content ─────────────────────────────
                {
                    "id": "settings_tab",
                    "component": "Column",
                    "children": [
                        "settings_title",
                        "theme_label",
                        "notif_label"
                    ],
                    "justify": "start",
                    "align": "stretch"
                },
                {
                    "id": "settings_title",
                    "component": "Text",
                    "text": "Settings",
                    "variant": "h3"
                },
                {
                    "id": "theme_label",
                    "component": "Text",
                    "text": {
                        "call": "formatString",
                        "args": {
                            "value": "Theme: ${/theme}"
                        }
                    },
                    "variant": "body"
                },
                {
                    "id": "notif_label",
                    "component": "Text",
                    "text": {
                        "call": "formatString",
                        "args": {
                            "value": "Notifications: ${/notifications}"
                        }
                    },
                    "variant": "body"
                },

                // ── Profile tab content ──────────────────────────────
                {
                    "id": "profile_tab",
                    "component": "Column",
                    "children": [
                        "profile_title",
                        "profile_name",
                        "profile_email",
                        "profile_date"
                    ],
                    "justify": "start",
                    "align": "stretch"
                },
                {
                    "id": "profile_title",
                    "component": "Text",
                    "text": "Profile",
                    "variant": "h3"
                },
                {
                    "id": "profile_name",
                    "component": "Text",
                    "text": {
                        "call": "formatString",
                        "args": {
                            "value": "Name: ${/username}"
                        }
                    },
                    "variant": "body"
                },
                {
                    "id": "profile_email",
                    "component": "Text",
                    "text": {
                        "call": "formatString",
                        "args": {
                            "value": "Email: ${/email}"
                        }
                    },
                    "variant": "body"
                },
                {
                    "id": "profile_date",
                    "component": "Text",
                    "text": {
                        "call": "formatDate",
                        "args": {
                            "value": {"path": "/currentDate"},
                            "format": "MMM dd, yyyy hh:mm a"
                        }
                    },
                    "variant": "caption"
                },

                // ── About tab content ────────────────────────────────
                {
                    "id": "about_tab",
                    "component": "Column",
                    "children": [
                        "about_title",
                        "about_text",
                        "about_version"
                    ],
                    "justify": "start",
                    "align": "stretch"
                },
                {
                    "id": "about_title",
                    "component": "Text",
                    "text": "About",
                    "variant": "h3"
                },
                {
                    "id": "about_text",
                    "component": "Text",
                    "text": "A2UI is a protocol for building terminal UIs from structured JSON messages.",
                    "variant": "body"
                },
                {
                    "id": "about_version",
                    "component": "Text",
                    "text": {
                        "call": "formatString",
                        "args": {
                            "value": "Version: ${/version}"
                        }
                    },
                    "variant": "caption"
                },

                // ── Divider ──────────────────────────────────────────
                {
                    "id": "divider_1",
                    "component": "Divider",
                    "axis": "horizontal"
                },

                // ── Modal ────────────────────────────────────────────
                {
                    "id": "help_modal",
                    "component": "Modal",
                    "isOpen": {"path": "/showHelp"},
                    "trigger": "help_button",
                    "content": "help_panel"
                },
                {
                    "id": "help_btn_label",
                    "component": "Text",
                    "text": "Show Help"
                },
                {
                    "id": "help_button",
                    "component": "Button",
                    "child": "help_btn_label",
                    "variant": "primary",
                    "action": {
                        "event": {
                            "name": "toggle_help",
                            "context": {}
                        }
                    }
                },
                {
                    "id": "help_panel",
                    "component": "Card",
                    "child": "help_content"
                },
                {
                    "id": "help_content",
                    "component": "Column",
                    "children": [
                        "help_title",
                        "help_line1",
                        "help_line2",
                        "help_line3",
                        "help_close_label",
                        "help_close_btn"
                    ],
                    "justify": "start",
                    "align": "stretch"
                },
                {
                    "id": "help_title",
                    "component": "Text",
                    "text": "Help",
                    "variant": "h3"
                },
                {
                    "id": "help_line1",
                    "component": "Text",
                    "text": "Left/Right: switch tabs when Tabs is focused",
                    "variant": "body"
                },
                {
                    "id": "help_line2",
                    "component": "Text",
                    "text": "Enter: activate button / toggle checkbox",
                    "variant": "body"
                },
                {
                    "id": "help_line3",
                    "component": "Text",
                    "text": "Tab/Shift+Tab: navigate between components",
                    "variant": "body"
                },
                {
                    "id": "help_close_label",
                    "component": "Text",
                    "text": "Close Help"
                },
                {
                    "id": "help_close_btn",
                    "component": "Button",
                    "child": "help_close_label",
                    "variant": "primary",
                    "action": {
                        "event": {
                            "name": "toggle_help",
                            "context": {}
                        }
                    }
                },

                // ── Footer hint ──────────────────────────────────────
                {
                    "id": "tab_hint",
                    "component": "Text",
                    "text": "Tab: focus  Left/Right: switch tabs  Enter: activate  q: quit",
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string())?)?;

    // ── 3. Set up focus management and terminal ──────────────────────────
    if let Some(surface) = processor.model.get_surface("tabs_demo") {
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

            if let Some(surface) = processor.model.get_surface("tabs_demo") {
                let renderer = a2ui::tui::surface::SurfaceRenderer::new(
                    surface, &registry, &render_catalog,
                );
                let focused = focus_manager.focused_id();
                renderer.render(frame, chunks[0], focused);

                // Build help bar.
                let dm = surface.data_model.borrow();
                let tab = dm.get("/activeTab").and_then(|v| v.as_u64()).unwrap_or(0);
                let show = dm.get("/showHelp").and_then(|v| v.as_bool()).unwrap_or(false);
                let tabs = ["Settings", "Profile", "About"];
                let tab_name = tabs.get(tab as usize).unwrap_or(&"?");
                drop(dm);

                let help_text = format!(
                    " Tab: {} | help: {} | Tab/Shift+Tab: navigate | q: quit ",
                    tab_name,
                    if show { "visible" } else { "hidden" },
                );
                let bar = Paragraph::new(Line::from(help_text))
                    .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(bar, chunks[1]);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => focus_manager.focus_next(),
                    KeyCode::BackTab => focus_manager.focus_prev(),
                    other => {
                        // Phase 1: dispatch event to focused component (immutable borrow).
                        let result = processor.model.get_surface("tabs_demo").and_then(|surface| {
                            dispatch_to_focused(
                                other,
                                "tabs_demo",
                                surface,
                                &registry,
                                &render_catalog,
                                &focus_manager,
                            )
                        });

                        // Phase 2: process the result (mutable borrow of processor).
                        if let Some(result) = result {
                            match result {
                                EventResult::DataUpdate { path, value } => {
                                    let msg = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "tabs_demo",
                                            "path": path,
                                            "value": value,
                                        }
                                    });
                                    let _ = processor.process_message(
                                        MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                                    );
                                }
                                EventResult::Toggle { path } => {
                                    let current = processor.model.get_surface("tabs_demo")
                                        .map(|s| {
                                            let dm = s.data_model.borrow();
                                            dm.get(&path).and_then(|v| v.as_bool()).unwrap_or(false)
                                        })
                                        .unwrap_or(false);
                                    let msg = serde_json::json!({
                                        "version": "v1.0",
                                        "updateDataModel": {
                                            "surfaceId": "tabs_demo",
                                            "path": path,
                                            "value": !current,
                                        }
                                    });
                                    let _ = processor.process_message(
                                        MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                                    );
                                }
                                EventResult::Action { event_name, context, .. } => {
                                    eprintln!("[ACTION] {} {:?}", event_name, context);

                                    // If the action is "toggle_help", flip /showHelp.
                                    if event_name == "toggle_help" {
                                        let show = processor.model.get_surface("tabs_demo")
                                            .map(|s| {
                                                let dm = s.data_model.borrow();
                                                dm.get("/showHelp").and_then(|v| v.as_bool()).unwrap_or(false)
                                            })
                                            .unwrap_or(false);
                                        let msg = serde_json::json!({
                                            "version": "v1.0",
                                            "updateDataModel": {
                                                "surfaceId": "tabs_demo",
                                                "path": "/showHelp",
                                                "value": !show,
                                            }
                                        });
                                        let _ = processor.process_message(
                                            MessageProcessor::parse_message(&msg.to_string()).unwrap(),
                                        );
                                    }
                                }
                                EventResult::Consumed => {}
                            }

                            // Rebuild focus after modal toggle may change component visibility.
                            if let Some(surface) = processor.model.get_surface("tabs_demo") {
                                let components = surface.components.borrow();
                                focus_manager.rebuild_from_components(&components);
                            }
                        }
                    }
                }
            }
        }
    }

    // ── 5. Cleanup ───────────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    if let Some(surface) = processor.model.get_surface("tabs_demo") {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}
