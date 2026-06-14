//! Binary entry point so `cargo install a2ui` runs the gallery app.
//!
//! This mirrors the pre-workspace-split behavior, where the single crate's
//! `main.rs` *was* the gallery. The umbrella crate is otherwise a pure
//! re-export library (`lib.rs`); this `main.rs` adds the installable binary.

use a2ui_gallery::app::GalleryApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = GalleryApp::new()?;
    app.run()?;
    Ok(())
}
