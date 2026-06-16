//! # Example: Capitalized Text (Reactive Function Binding)
//!
//! The A2UI "Capitalized Text" sample — a `TextField` whose value is bound to
//! `/inputValue`, and a `Text` whose content is a **declarative function call**
//! `capitalize(/inputValue)`. As the user types, the data model updates and the
//! dependent `Text` reactively re-evaluates `capitalize` on every render.
//!
//! This is the canonical test of A2UI "Reactive Logic" — changes in one
//! component dynamically update a dependent, function-bound component. It
//! requires the **minimal** catalog (the only catalog that ships the
//! `capitalize` function); the basic catalog does not.
//!
//! ## What it demonstrates
//! - A component property (`text`) set to a function call, not a literal/binding
//! - `capitalize` resolving a nested `{"path": "/inputValue"}` argument
//! - Two-way `TextField` binding writing keystrokes back to `/inputValue`
//! - Immediate-mode re-rendering: the capitalized output updates live as you type
//!
//! ## Run
//! ```sh
//! cargo run --example 19_capitalized_text
//! ```
//!
//! ## Test
//! ```sh
//! cargo test --example 19_capitalized_text
//! ```
//!
//! ## Keys
//! - Type any character → appended to the input, output re-capitalized
//! - `Backspace` → delete the last character
//! - `Tab` / `Shift+Tab` → move focus (only one field here)
//! - `q` → quit

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
use a2ui::core::event::InputEvent;
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::minimal::{build_minimal_catalog, build_minimal_registry};
use a2ui::tui::component_impl::ComponentRegistry;
use a2ui::tui::focus_manager::FocusManager;
use a2ui::tui::interaction;

/// Surface id used by this example.
const SURFACE_ID: &str = "capitalized";

/// Shared application state, built once by [`build_state`] and then driven by
/// both the live terminal loop (`main`) and the end-to-end test.
struct State {
    processor: MessageProcessor,
    registry: ComponentRegistry,
    catalog: Catalog,
    focus: FocusManager,
}

/// Build the Capitalized Text surface: minimal catalog + a `TextField` bound to
/// `/inputValue` and a `Text` whose `text` is the `capitalize(/inputValue)`
/// function call. Focus is rebuilt so the `TextField` is focused immediately.
fn build_state() -> Result<State, Box<dyn std::error::Error>> {
    let registry = build_minimal_registry();
    let catalog = build_minimal_catalog();
    let mut processor = MessageProcessor::new(vec![build_minimal_catalog()]);

    // `/inputValue` starts empty; the TextField writes to it and the capitalize
    // Text reads from it.
    let create_msg = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": SURFACE_ID,
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json",
            "dataModel": { "inputValue": "" }
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create_msg.to_string()).unwrap())?;

    // `result_text` is the key piece: its `text` is a function call whose
    // `value` argument is itself a binding to `/inputValue`. The renderer
    // resolves it fresh on every frame, so it tracks the field live.
    let update_msg = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": SURFACE_ID,
            "components": [
                {
                    "id": "root",
                    "component": "Column",
                    "children": ["prompt", "input_field", "result_label", "result_text", "hint"],
                    "justify": "center",
                    "align": "stretch"
                },
                {
                    "id": "prompt",
                    "component": "Text",
                    "text": "Type something in lowercase:",
                    "variant": "h2"
                },
                {
                    "id": "input_field",
                    "component": "TextField",
                    "label": "Input",
                    "value": {"path": "/inputValue"},
                    "variant": "shortText"
                },
                {
                    "id": "result_label",
                    "component": "Text",
                    "text": "Capitalized output:",
                    "variant": "caption"
                },
                {
                    "id": "result_text",
                    "component": "Text",
                    "text": {
                        "call": "capitalize",
                        "args": { "value": {"path": "/inputValue"} },
                        "returnType": "string"
                    },
                    "variant": "h1"
                },
                {
                    "id": "hint",
                    "component": "Text",
                    "text": "Tab: focus  |  Backspace: delete  |  q: quit",
                    "variant": "caption"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&update_msg.to_string()).unwrap())?;

    // The TextField is the only focusable component, so after rebuild it is the
    // focused one and will receive keystrokes.
    let mut focus = FocusManager::new();
    if let Some(surface) = processor.model.get_surface(SURFACE_ID) {
        let components = surface.components.borrow();
        focus.rebuild_from_components(&components);
    }

    Ok(State { processor, registry, catalog, focus })
}

/// Send a printable character or editing key (e.g. `Backspace`) to the focused
/// component and apply the resulting data update. Returns `true` if a component
/// handled the key.
///
/// The `TextField`'s `handle_event` returns a `DataUpdate` writing the new value
/// back to `/inputValue`; `apply_event_result` commits it. The next render then
/// re-evaluates `capitalize(/inputValue)` for `result_text`.
fn send_key(state: &mut State, code: KeyCode) -> bool {
    let Some(key) = interaction::map_key_code(code) else {
        return false;
    };
    let event = InputEvent::KeyPress { key };
    let result = interaction::dispatch_to_focused(
        &state.processor,
        &state.registry,
        &state.catalog,
        &state.focus,
        &event,
    );
    if let Some(result) = result {
        interaction::apply_event_result(&mut state.processor, result);
        true
    } else {
        false
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = build_state()?;

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

            if let Some(surface) = state.processor.model.get_surface(SURFACE_ID) {
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &state.registry, &state.catalog);
                let focused = state.focus.focused_id();
                renderer.render(frame, chunks[0], focused);

                // Echo the raw data-model value so the two-way binding is visible.
                let raw = surface
                    .data_model
                    .borrow()
                    .get("/inputValue")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let bar = Paragraph::new(Line::from(format!(
                    " /inputValue = {:?}   (function-bound Text reactively re-capitalizes this) ",
                    raw
                )))
                .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(bar, chunks[1]);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => state.focus.focus_next(),
                    KeyCode::BackTab => state.focus.focus_prev(),
                    other => {
                        send_key(&mut state, other);
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    if let Some(surface) = state.processor.model.get_surface(SURFACE_ID) {
        let dm = surface.data_model.borrow();
        println!("Final data model: {}", serde_json::to_string_pretty(&dm.as_value())?);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// End-to-end test
// ---------------------------------------------------------------------------
// Drives the *whole* stack the live app uses — message parsing, the minimal
// catalog's `capitalize` function, the `TextField` two-way binding, the
// interaction dispatch pipeline, and the `SurfaceRenderer` — but renders into a
// ratatui `TestBackend` buffer so we can assert the function-bound `Text`
// reactively re-capitalizes `/inputValue` as keys arrive. No real terminal,
// no PTY: deterministic.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// Render the Capitalized Text surface into a fresh `cols x rows`
    /// `TestBackend` buffer, mirroring the live app's draw call.
    fn render_buffer(state: &State, cols: u16, rows: u16) -> ratatui::buffer::Buffer {
        let surface = state
            .processor
            .model
            .get_surface(SURFACE_ID)
            .expect("surface exists");
        let backend = TestBackend::new(cols, rows);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &state.registry, &state.catalog);
                renderer.render(frame, frame.area(), state.focus.focused_id());
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    /// Flatten a buffer into a single string (all cell symbols, row by row) so
    /// assertions can use plain `contains`.
    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        let area = buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    /// Read `/inputValue` straight from the data model.
    fn input_value(state: &State) -> String {
        let surface = state.processor.model.get_surface(SURFACE_ID).unwrap();
        surface
            .data_model
            .borrow()
            .get("/inputValue")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn empty_input_capitalizes_to_nothing() {
        let state = build_state().unwrap();
        let text = buffer_text(&render_buffer(&state, 80, 24));
        // capitalize("") == "" — nothing rendered for the output line yet.
        assert!(!text.contains("Hello"));
        assert_eq!(input_value(&state), "");
    }

    #[test]
    fn capitalize_reacts_live_as_you_type() {
        let mut state = build_state().unwrap();

        // Type "hello" one character at a time; after each keystroke the
        // function-bound Text must re-capitalize the growing value.
        let checks = [
            ('h', "H"),
            ('e', "He"),
            ('l', "Hel"),
            ('l', "Hell"),
            ('o', "Hello"),
        ];
        for (ch, expected) in checks {
            assert!(send_key(&mut state, KeyCode::Char(ch)), "key handled");
            let text = buffer_text(&render_buffer(&state, 80, 24));
            assert!(
                text.contains(expected),
                "after typing {ch:?}: expected {expected:?} in render, got:\n{text}"
            );
        }
        assert_eq!(input_value(&state), "hello");
    }

    #[test]
    fn backspace_updates_capitalized_output() {
        let mut state = build_state().unwrap();
        for ch in "hello".chars() {
            send_key(&mut state, KeyCode::Char(ch));
        }
        assert_eq!(input_value(&state), "hello");

        // Backspace drops the trailing 'o' -> "hell" -> "Hell".
        assert!(send_key(&mut state, KeyCode::Backspace));
        let text = buffer_text(&render_buffer(&state, 80, 24));
        assert!(text.contains("Hell"), "expected 'Hell':\n{text}");
        assert!(!text.contains("Hello"), "should no longer be 'Hello':\n{text}");
        assert_eq!(input_value(&state), "hell");
    }

    #[test]
    fn capitalize_only_affects_first_character() {
        // Guard the capitalize contract: the rest of the string is untouched.
        let mut state = build_state().unwrap();
        for ch in "hELLO".chars() {
            send_key(&mut state, KeyCode::Char(ch));
        }
        let text = buffer_text(&render_buffer(&state, 80, 24));
        assert!(text.contains("HELLO"), "expected 'HELLO' (only first char uppercased):\n{text}");
        assert_eq!(input_value(&state), "hELLO");
    }
}
