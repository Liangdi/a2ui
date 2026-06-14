//! Keyboard focus management for the TUI renderer.
//!
//! Moved to `a2ui_core::focus` (framework-agnostic) so every backend shares one
//! implementation. This module re-exports it under the historical
//! `a2ui_tui::focus_manager` path so existing imports keep working.

pub use a2ui_core::focus::{FocusManager, FOCUSABLE_TYPES};
