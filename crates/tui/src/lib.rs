//! Ratatui terminal backend for A2UI — the default renderer (`a2ui-tui`).
//!
//! Translates an A2UI component tree (the flat `id → ComponentModel` map owned by
//! [`a2ui_base::model`]) into ratatui widgets painted on a character grid, and
//! bridges terminal key events back to the framework-agnostic interaction layer
//! in [`a2ui_base`].
//!
//! Unlike the desktop backends, ratatui is **immediate-mode on a character grid**:
//! every frame the whole tree is re-walked and re-painted, with a manual measure
//! pass (see [`surface`]) for sizing layout containers. There are no pixels, so
//! `Image` / `Video` / `AudioPlayer` render text placeholders by default and only
//! decode real images behind a cargo feature flag.
//!
//! [`surface`] is the entry point — [`SurfaceRenderer`] walks a `SurfaceModel` and
//! renders it into a ratatui `Frame`.
//!
//! [`SurfaceRenderer`]: surface::SurfaceRenderer

pub mod catalogs;
pub mod component_impl;
pub mod components;
pub mod focus_manager;
pub mod interaction;
pub mod layout_engine;
pub mod surface;
// Shared "agent chat" scenario builders (mock AI agent → A2UI protocol message
// streams). Framework-agnostic JSON, so every UI backend's `08_agent_chat`
// example imports this instead of duplicating the scenarios. Lives here, next to
// `catalogs::basic`, because that catalog builder is likewise shared by every
// backend example.
pub mod agent_chat;
