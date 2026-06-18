//! # Example: `a2ui-json/` scenario gallery
//!
//! Browses and interactively renders the ad-hoc scenario JSON files in the
//! workspace-root [`a2ui-json/`] directory, so new scenarios can be dropped in
//! and visually verified against the ratatui backend.
//!
//! Unlike the embedded spec samples (wrapped `{name, description, messages}`),
//! these files are bare JSON arrays of messages — both formats are auto-detected
//! by [`a2ui_gallery::sample_loader::parse_scenario`].
//!
//! Reuses the full [`GalleryApp`] interaction surface: `↑/↓` walk scenarios,
//! `Enter` open, `n` step messages, `a` show all, `r` replay, `Tab` cycle focus,
//! `t` cycle theme, `q`/`Esc` quit.
//!
//! [`a2ui-json/`]: https://github.com/Liangdi/a2ui/tree/master/a2ui-json
//!
//! ## Run
//! ```sh
//! # default: <workspace>/a2ui-json
//! cargo run --example json_gallery -p a2ui-gallery
//!
//! # explicit directory
//! cargo run --example json_gallery -p a2ui-gallery -- ./a2ui-json
//!
//! # via env var
//! A2UI_JSON_DIR=./a2ui-json cargo run --example json_gallery -p a2ui-gallery
//! ```

use a2ui_gallery::app::GalleryApp;
use a2ui_gallery::sample_loader::load_scenarios;

/// Compile-time default scenario directory: the workspace-root `a2ui-json/`,
/// two levels above this crate's manifest dir (`crates/gallery`).
const DEFAULT_JSON_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../a2ui-json");

/// Resolve the scenario directory in priority order: CLI arg → `A2UI_JSON_DIR`
/// env var → the compile-time default.
fn resolve_json_dir() -> String {
    if let Some(arg) = std::env::args().nth(1) {
        return arg;
    }
    if let Ok(dir) = std::env::var("A2UI_JSON_DIR") {
        return dir;
    }
    DEFAULT_JSON_DIR.to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = resolve_json_dir();
    let scenarios = load_scenarios(&dir);

    // Printed to stdout BEFORE entering raw mode (the TUI paints stderr), so the
    // user gets immediate feedback without corrupting the alternate screen.
    println!("a2ui-json scenario gallery");
    println!("  directory: {dir}");
    println!("  scenarios: {}", scenarios.len());

    if scenarios.is_empty() {
        eprintln!(
            "No scenarios found in {dir:?}. Drop `.json` files there (or pass a \
             directory argument / set A2UI_JSON_DIR) and re-run."
        );
        return Ok(());
    }

    let mut app = GalleryApp::with_samples(scenarios)?;
    app.run()?;
    Ok(())
}
