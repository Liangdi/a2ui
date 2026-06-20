//! Framework-agnostic core of the [A2UI] protocol — the `a2ui-base` crate.
//!
//! [A2UI] (Agent to UI) v1.0 is a JSON streaming UI protocol that lets an AI
//! agent dynamically generate and update an interface. This crate holds
//! everything that is **not** tied to a specific renderer, so every backend
//! (ratatui terminal, Slint, egui, Bevy, Iced, Dioxus) is built on top of it:
//!
//! - [`protocol`] — wire types for the client ⇄ server message stream.
//! - [`model`] — the runtime component tree, data model, and JSON-Pointer binding.
//! - [`catalog`] — component APIs, function implementations, and the [`Catalog`].
//! - [`message_processor`] — the streaming message → model-update pipeline.
//! - [`validate`] — integrity / topology / recursion checks plus a fault-tolerant
//!   `parse_and_fix` for untrusted or LLM-generated payloads.
//! - [`capabilities`] — client/server capability negotiation and inline catalogs.
//! - [`focus`] / [`interaction`] — keyboard-focus traversal and event application
//!   shared by every backend.
//! - [`components`] — backend-agnostic `handle_event` behavior for interactive types.
//!
//! Nothing here renders anything — pick a backend (`a2ui-tui` is the default) to
//! put a tree on screen.
//!
//! [A2UI]: https://github.com/a2ui-project/a2ui
//!
//! [`Catalog`]: catalog::Catalog

pub mod capabilities;
pub mod catalog;
pub mod error;
pub mod event;
pub mod message_processor;
pub mod model;
pub mod observable;
pub mod protocol;
pub mod validate;
// Framework-agnostic interaction layer, shared by every UI backend.
// `focus` is keyboard-focus traversal over the component tree; `interaction`
// applies a component's EventResult to the runtime state. Each backend maps its
// own key enum to InputKey and dispatches to components itself.
pub mod focus;
pub mod interaction;
// Framework-agnostic component **behavior** (the `handle_event` logic) for the
// interactive types whose handlers have no backend coupling. Each UI backend
// reuses these instead of duplicating per-component key handling.
pub mod components;
