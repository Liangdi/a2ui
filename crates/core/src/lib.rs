pub mod error;
pub mod event;
pub mod protocol;
pub mod model;
pub mod observable;
pub mod catalog;
pub mod message_processor;
pub mod capabilities;
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
