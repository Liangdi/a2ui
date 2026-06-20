//! # Example: Interactive AudioPlayer Component (`audio` feature)
//!
//! Renders the real A2UI `AudioPlayer` component through `SurfaceRenderer`
//! and drives it with the keyboard — demonstrating that the component itself
//! is interactive (play/pause, volume, replay) when the `audio` feature is on.
//! Playback starts automatically on first render; subsequent renders reuse
//! the live `rodio` handle (no per-frame re-trigger).
//!
//! `Tab` to focus the player, then:
//!
//! | Key | Action |
//! |-----|--------|
//! | `Space` | Play / Pause (or Replay when finished) |
//! | `↑` / `↓` | Volume ±10 % |
//! | `Tab` / `Shift+Tab` | Cycle focus |
//! | `q` / `Esc` | Quit |
//!
//! Requires the `audio` Cargo feature and the ALSA system dev library
//! (`alsa-lib-devel` on Fedora / `libasound2-dev` on Debian). Defaults to
//! `examples/assets/sample.wav`; pass a path as the first CLI argument.
//!
//! ## Run
//! ```sh
//! cargo run --example 14_audio --features audio
//! cargo run --example 14_audio --features audio -- /path/to/sound.wav
//! ```

use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
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

use a2ui::core::catalog::Catalog;
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui::tui::focus_manager::FocusManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── 1. Resolve the audio path ────────────────────────────────────────
    let default_path = format!("{}/examples/assets/sample.wav", env!("CARGO_MANIFEST_DIR"));
    let audio_path = std::env::args().nth(1).unwrap_or(default_path);
    if !std::path::Path::new(&audio_path).is_file() {
        return Err(format!("audio file not found: {audio_path}").into());
    }

    // ── 2. Build a surface whose root is an AudioPlayer component ─────────
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    let create = serde_json::json!({
        "version": "v1.0",
        "createSurface": {
            "surfaceId": "player",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
        }
    });
    let update = serde_json::json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "player",
            "components": [
                {
                    "id": "root",
                    "component": "AudioPlayer",
                    "url": audio_path,
                    "description": "A2UI Audio Demo"
                }
            ]
        }
    });
    processor.process_message(MessageProcessor::parse_message(&create.to_string())?)?;
    processor.process_message(MessageProcessor::parse_message(&update.to_string())?)?;

    // ── 3. Focus manager (AudioPlayer is in the focusable set) ───────────
    let mut focus = FocusManager::new();
    {
        let surface = processor.model.surfaces().next().expect("surface exists");
        let components = surface.components.borrow();
        focus.rebuild_from_components(&components);
    }

    // ── 4. Terminal + event loop ─────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    loop {
        let focused_id = focus.focused_id().map(|s| s.to_string());

        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(1)])
                .split(area);

            if let Some(surface) = processor.model.surfaces().next() {
                let renderer =
                    a2ui::tui::surface::SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, chunks[0], focused_id.as_deref());
            }

            let bar = Paragraph::new(Line::from(
                " Tab:focus   Space:play/pause/replay   \u{2191}/\u{2193}:volume   q:quit",
            ))
            .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(bar, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Tab => focus.focus_next(),
                    KeyCode::BackTab => focus.focus_prev(),
                    code => {
                        a2ui::tui::interaction::handle_key(
                            &mut processor,
                            &registry,
                            &render_catalog,
                            &focus,
                            code,
                        );
                    }
                }
            }
        }
    }

    // ── 5. Restore the terminal ──────────────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    println!("Goodbye! (played via the interactive AudioPlayer component)");
    Ok(())
}
