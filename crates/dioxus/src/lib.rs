//! Dioxus backend for A2UI.
//!
//! Translates an A2UI component tree (the flat `id → ComponentModel` map owned
//! by [`a2ui_base::model`]) into a Dioxus [`Element`] tree rendered into a
//! desktop WebView, and bridges widget interactions back to the
//! framework-agnostic interaction layer in `a2ui_base` via Dioxus's
//! reactive-signals architecture.
//!
//! Of the six A2UI renderers this is the most architecturally distinct. Dioxus
//! is a *reactive-signals* framework (like React): the runtime state lives in a
//! [`Signal`] at the root, and the UI is a pure read of it — so unlike the Iced
//! backend (Elm `view`/`update`, needs a `Message` enum) or the egui backend
//! (immediate mode, needs a persistent `EditBuffers` state bridge), interactive
//! widgets here read straight from the signal in render and write straight back
//! through it on interaction. **No message enum, no state bridge.** The signal
//! *is* the interaction channel: any write re-renders every component that read
//! it.
//!
//! Two further Dioxus-specific simplifications:
//! - **Recursive components** — the whole tree is one [`A2uiNode`] component
//!   that renders itself per node (Dioxus supports recursion natively, unlike
//!   Slint's bounded-depth codegen), so there is no flat-array workaround.
//! - **WebView rendering** — Dioxus desktop renders to a system WebView
//!   (WebKitGTK on Linux), so the bespoke dark theme is a CSS stylesheet
//!   ([`theme::STYLESHEET`]) rather than a per-widget style-fn palette, and the
//!   A2UI component kinds map to ordinary HTML elements + classes.
//!
//! Everything here lives behind the `backend` cargo feature, which pulls in the
//! Dioxus desktop runtime. Without it this crate is an empty shell (it compiles
//! with no dependencies beyond `a2ui-base`), keeping the workspace's default
//! build light.
//!
//! [`Element`]: dioxus::Element
//! [`Signal`]: dioxus::prelude::Signal

#![cfg_attr(not(feature = "backend"), allow(unused_imports))]

#[cfg(feature = "backend")]
pub mod app;
#[cfg(feature = "backend")]
pub mod node;
#[cfg(feature = "backend")]
pub mod theme;

/// The gallery root component — owns the state signals and renders the sidebar
/// + preview pane + modal overlay. Construct from the gallery (or any host)
/// and hand to `dioxus::launch`.
#[cfg(feature = "backend")]
pub use app::Gallery;

/// The recursive per-node component. Re-exported so a host can render a raw
/// subtree (`<A2uiNode id=.. base_path=.. />`) without the gallery chrome.
#[cfg(feature = "backend")]
pub use node::A2uiNode;

/// The whole-gallery CSS stylesheet (the dark Catppuccin-Mocha + green-accent
/// palette). Inject it via the desktop `Config`'s custom `<head>`.
#[cfg(feature = "backend")]
pub use theme::STYLESHEET;

/// Re-export the core interaction pieces backends compose against, so consumers
/// can `use a2ui_dioxus::{dispatch_event, apply_event_result, ...}` in one place.
#[cfg(feature = "backend")]
pub use a2ui_base::components::dispatch_event;
#[cfg(feature = "backend")]
pub use a2ui_base::focus::FocusManager;
#[cfg(feature = "backend")]
pub use a2ui_base::interaction::apply_event_result;
