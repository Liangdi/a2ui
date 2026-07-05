//! GPUI backend for A2UI.
//!
//! Translates an A2UI component tree (the flat `id → ComponentModel` map owned
//! by [`a2ui_base::model`]) into a [GPUI] element tree and bridges widget
//! interactions back to the framework-agnostic interaction layer in
//! `a2ui_base`, rendering through the [gpui-component] widget set.
//!
//! GPUI is Zed's hybrid immediate/retained-mode, GPU-accelerated UI framework.
//! Like egui it rebuilds the element tree each frame from a `render` method,
//! but — unlike egui's string-keyed `EditBuffers` — GPUI's **stateful** widgets
//! (text input, slider, select) own a persistent [`Entity<…State>`] that
//! survives across frames. So this backend keeps a per-component-id cache of
//! `Entity<InputState>` / `Entity<SliderState>` / `Entity<SelectState>` (lazily
//! created on first render, seeded from the data model, subscribed to their
//! change events for write-back). Stateless interactive widgets — Button,
//! Checkbox, TabBar, list rows — attach `cx.listener` closures that fire
//! *after* the render borrow is released and mutate the runtime directly, so no
//! egui-style collect-then-apply buffer is needed (closer to iced's direct
//! `update`, but deferred by one event loop tick).
//!
//! The recursive walker in [`walker`] builds the element tree, dispatching to
//! the matching `render_*` arm in [`components`] by component type — the GPUI
//! counterpart of the iced/egui/ratatui `render_node`.
//!
//! Everything here lives behind the `backend` cargo feature, which pulls in the
//! GPUI runtime + gpui-component widget set. Without it this crate is an empty
//! shell (it compiles with no dependencies beyond `a2ui-base`), keeping the
//! workspace's default build light.
//!
//! [GPUI]: https://gpui.rs
//! [gpui-component]: https://github.com/longbridge/gpui-component

#![cfg_attr(not(feature = "backend"), allow(unused_imports))]

#[cfg(feature = "backend")]
pub mod app;
#[cfg(feature = "backend")]
pub mod components;
#[cfg(feature = "backend")]
pub mod images;
#[cfg(feature = "backend")]
pub mod theme;
#[cfg(feature = "backend")]
pub mod walker;

/// The GPUI app view — owns the surface state and implements GPUI's `Render`
/// trait (rebuilt each frame). Construct from the gallery (or any host) and
/// hand to `cx.open_window` wrapped in a `gpui_component::Root`.
#[cfg(feature = "backend")]
pub use app::GpuiApp;

/// Re-export the core interaction pieces backends compose against, so consumers
/// can `use a2ui_gpui::{dispatch_event, apply_event_result, ...}` in one place.
#[cfg(feature = "backend")]
pub use a2ui_base::components::dispatch_event;
#[cfg(feature = "backend")]
pub use a2ui_base::focus::FocusManager;
#[cfg(feature = "backend")]
pub use a2ui_base::interaction::apply_event_result;

/// Customize the gpui-component global theme (dark mode + the gallery's green
/// accent) for the host's launch closure. Re-exported from [`theme`].
#[cfg(feature = "backend")]
pub use theme::customize;
