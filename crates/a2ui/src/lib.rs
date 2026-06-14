//! Umbrella crate that re-exports the A2UI backends under the historical
//! `a2ui::core` / `a2ui::tui` paths, so existing `use a2ui::core::...` and
//! `use a2ui::tui::...` imports keep working after the workspace split.
//!
//! The Slint backend is available as `a2ui::slint` under the `slint` cargo
//! feature — it is opt-in because it pulls the (heavy) Slint runtime.

pub use a2ui_core as core;
pub use a2ui_tui as tui;

#[cfg(feature = "slint")]
pub use a2ui_slint as slint;
