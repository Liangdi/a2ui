//! Bevy backend for A2UI.
//!
//! Translates an A2UI component tree (the flat `id → ComponentModel` map owned
//! by [`a2ui_base::model`]) into a retained Bevy UI entity tree and bridges Bevy
//! widget interactions back to the framework-agnostic interaction layer in
//! [`a2ui_base`].
//!
//! ## How it differs from the other backends
//!
//! - **ratatui** paints immediate-mode on a character grid with a manual measure pass.
//! - **slint** builds a retained live tree but **rebuilds it wholesale every frame**
//!   (no entity identity) — fine for Slint, whose widgets manage their own state.
//! - **egui** is immediate-mode and recurses a fresh tree each frame, carrying
//!   widget state in an [`egui`-style] `EditBuffers` map keyed by component id.
//! - **bevy** is ECS retained-mode. Bevy's interactive widgets (`bevy_ui_widgets`
//!   Button/Checkbox/Slider and the external `bevy_ui_text_input`) only keep
//!   correct drag/hover/focus/cursor state when their **entity identity is
//!   preserved across frames** — a per-frame rebuild (Slint's approach) would
//!   fling sliders and drop cursors every frame. So this backend introduces a
//!   **React-style reconciler**: it keeps a stable `HashMap<component_id,
//!   Entity>` ([`state::A2uiState`]'s `node_map`) and spawn/update/despawn/
//!   reorder incrementally each frame. Because the text-input entity persists,
//!   it owns its cursor + edit state — no `EditBuffers` map is needed.
//!
//! ## The render loop (Bevy systems, in order)
//!
//! 1. [`interaction::collect_interactions`] — read widget events
//!    (`Activate` / `ValueChange<T>` from `bevy_ui_widgets`; text-input buffer
//!    diffs), map source `Entity` → A2UI `component_id` via the [`state::A2uiNode`]
//!    marker, push a [`interaction::PendingInteraction`].
//! 2. [`interaction::apply_interactions`] — consume the pending list, mutate the
//!    `MessageProcessor` via the shared core pipeline, mark the tree dirty.
//! 3. [`reconcile::reconcile`] — walk the A2UI tree; spawn/update/despawn/reorder
//!    Bevy entities so the live tree mirrors the model, re-resolving dynamic
//!    properties each frame.
//!
//! Everything lives behind the `backend` cargo feature, which pulls the Bevy +
//! `bevy_ui_text_input` runtimes. Without it this crate is an empty shell
//! (compiles with no deps beyond `a2ui-base`), keeping the workspace's default
//! build light.

#![cfg_attr(not(feature = "backend"), allow(unused_imports))]

#[cfg(feature = "backend")]
pub mod images;
#[cfg(feature = "backend")]
pub mod interaction;
#[cfg(feature = "backend")]
pub mod plugin;
#[cfg(feature = "backend")]
pub mod reconcile;
#[cfg(feature = "backend")]
pub mod render;
#[cfg(feature = "backend")]
pub mod sample_browser;
#[cfg(feature = "backend")]
pub mod state;

/// The Bevy plugin — registers the render-loop systems + resources and spawns
/// the base UI (camera, root container, sample-browser panel). Add to a Bevy
/// `App` alongside `DefaultPlugins`.
#[cfg(feature = "backend")]
pub use plugin::A2uiPlugin;

/// The shared runtime state resource: owns the `MessageProcessor`, the function
/// map, the `FocusManager`, the locally-tracked open-modals set, and the stable
/// `node_map` the reconciler diffs against.
#[cfg(feature = "backend")]
pub use state::A2uiState;

/// Re-export the core interaction pieces backends compose against, so consumers
/// can `use a2ui_bevy::{dispatch_event, apply_event_result, ...}` in one place.
#[cfg(feature = "backend")]
pub use a2ui_base::components::dispatch_event;
#[cfg(feature = "backend")]
pub use a2ui_base::focus::FocusManager;
#[cfg(feature = "backend")]
pub use a2ui_base::interaction::apply_event_result;
