//! Slint backend for A2UI.
//!
//! Translates an A2UI component tree (the flat `id → ComponentModel` map owned
//! by [`a2ui_core::model`]) into a Slint reactive tree and bridges Slint UI
//! events back to the framework-agnostic interaction layer in `a2ui_core`.
//!
//! Unlike the ratatui backend (immediate-mode painting on a character grid),
//! Slint is retained-mode + declarative: layout is handled by Slint's engine,
//! so this backend does **not** reproduce the tui's measure pass or
//! `layout_engine`. Instead it walks the component tree into a [`live_tree`]
//! model that `.slint` components bind to reactively.
//!
//! Everything here lives behind the `backend` cargo feature, which pulls in the
//! Slint runtime. Without it this crate is an empty shell (it compiles with no
//! dependencies beyond `a2ui-core`), keeping the workspace's default build light.

#![cfg_attr(not(feature = "backend"), allow(unused_imports))]

#[cfg(feature = "backend")]
pub mod host;
#[cfg(feature = "backend")]
pub mod live_tree;
#[cfg(feature = "backend")]
pub mod ui;

/// The reactive-tree node type generated from `.slint` (see `build.rs`).
#[cfg(feature = "backend")]
pub use ui::LiveNode;

/// Re-export the core interaction pieces backends compose against, so consumers
/// can `use a2ui_slint::{dispatch_event, apply_event_result, ...}` in one place.
#[cfg(feature = "backend")]
pub use a2ui_core::components::dispatch_event;
#[cfg(feature = "backend")]
pub use a2ui_core::focus::FocusManager;
#[cfg(feature = "backend")]
pub use a2ui_core::interaction::apply_event_result;
