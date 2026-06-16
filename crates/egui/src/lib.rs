//! egui backend for A2UI.
//!
//! Translates an A2UI component tree (the flat `id → ComponentModel` map owned
//! by [`a2ui_base::model`]) into egui immediate-mode widgets and bridges egui
//! interactions back to the framework-agnostic interaction layer in `a2ui_base`.
//!
//! Unlike the ratatui backend (immediate-mode painting on a character grid with
//! a manual measure pass) and the Slint backend (retained-mode, forced into a
//! flat index-array + bounded-depth `Node0..N` chain because Slint can't
//! recurse), egui is **native-recursive** and **auto-layouts**: the walker in
//! [`walker`] recurses directly into a `&mut egui::Ui`, with no `build.rs` and
//! no measure pass. Interactive components (TextField/Slider/CheckBox/…) use
//! real egui widgets, bridged to the data-model-as-source-of-truth via the
//! persistent [`edit_state::EditBuffers`] map.
//!
//! Everything here lives behind the `backend` cargo feature, which pulls in the
//! egui + eframe runtime. Without it this crate is an empty shell (it compiles
//! with no dependencies beyond `a2ui-base`), keeping the workspace's default
//! build light.

#![cfg_attr(not(feature = "backend"), allow(unused_imports))]

#[cfg(feature = "backend")]
pub mod app;
#[cfg(feature = "backend")]
pub mod components;
#[cfg(feature = "backend")]
pub mod edit_state;
#[cfg(feature = "backend")]
pub mod images;
#[cfg(feature = "backend")]
pub mod interaction;
#[cfg(feature = "backend")]
pub mod walker;

/// The eframe app — owns the surface state and drives the immediate-mode render
/// loop. Construct from the gallery (or any host) and hand to `eframe`.
#[cfg(feature = "backend")]
pub use app::EguiApp;

/// Re-export the core interaction pieces backends compose against, so consumers
/// can `use a2ui_egui::{dispatch_event, apply_event_result, ...}` in one place.
#[cfg(feature = "backend")]
pub use a2ui_base::components::dispatch_event;
#[cfg(feature = "backend")]
pub use a2ui_base::focus::FocusManager;
#[cfg(feature = "backend")]
pub use a2ui_base::interaction::apply_event_result;
