//! Iced backend for A2UI.
//!
//! Translates an A2UI component tree (the flat `id → ComponentModel` map owned
//! by [`a2ui_base::model`]) into an [Iced] [`Element`] tree and bridges widget
//! interactions back to the framework-agnostic interaction layer in
//! `a2ui_base` via Iced's Elm architecture.
//!
//! Of the five A2UI renderers this is the cleanest mapping. Iced is Elm:
//! `view(&state)` returns an immutable [`Element`] tree built from the data
//! model, and `update(&mut state, message)` mutates state. So — unlike the
//! egui backend (immediate mode, needs a persistent [`EditBuffers`]-style
//! state bridge because the data model is borrowed for the whole frame) or the
//! bevy backend (retained ECS, needs a reconciler to diff/patch the entity
//! tree) — interactive widgets here read straight from the data model in
//! `view` and write back through a [`Message`] in `update`. **No state bridge,
//! no diffing.** Each widget that the user interacts with emits a [`Message`],
//! and [`IcedApp::update`] applies it once `view`'s borrows are dropped.
//!
//! The recursive walker in [`walker`] builds the element tree, dispatching to
//! the matching `render_*` arm in [`components`] by component type — the Iced
//! counterpart of the egui `render_node` and the ratatui `render_node`.
//!
//! Everything here lives behind the `backend` cargo feature, which pulls in the
//! Iced runtime. Without it this crate is an empty shell (it compiles with no
//! dependencies beyond `a2ui-base`), keeping the workspace's default build
//! light.
//!
//! [Iced]: https://iced.rs
//! [`EditBuffers`]: a2ui_egui::EditBuffers

#![cfg_attr(not(feature = "backend"), allow(unused_imports))]

#[cfg(feature = "backend")]
pub mod app;
#[cfg(feature = "backend")]
pub mod components;
#[cfg(feature = "backend")]
pub mod message;
#[cfg(feature = "backend")]
pub mod walker;

/// The Iced app — owns the surface state and drives the Elm
/// `view`/`update` loop. Construct from the gallery (or any host) and hand to
/// `iced::application`.
#[cfg(feature = "backend")]
pub use app::IcedApp;

/// The `Message` enum the backend's widgets emit. Re-exported so a host can
/// inspect / test the message stream.
#[cfg(feature = "backend")]
pub use message::Message;

/// Re-export the core interaction pieces backends compose against, so consumers
/// can `use a2ui_iced::{dispatch_event, apply_event_result, ...}` in one place.
#[cfg(feature = "backend")]
pub use a2ui_base::components::dispatch_event;
#[cfg(feature = "backend")]
pub use a2ui_base::focus::FocusManager;
#[cfg(feature = "backend")]
pub use a2ui_base::interaction::apply_event_result;
