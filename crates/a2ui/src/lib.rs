//! Umbrella crate that re-exports the A2UI backends under the historical
//! `a2ui::core` / `a2ui::tui` paths, so existing `use a2ui::core::...` and
//! `use a2ui::tui::...` imports keep working after the workspace split.

pub use a2ui_core as core;
pub use a2ui_tui as tui;
