//! The backend's `Message` enum — the Elm interaction channel.
//!
//! Iced is Elm: `view` builds an immutable element tree and each interactive
//! widget attaches a [`Message`] (via `.on_press` / `.on_input` / …) that
//! [`IcedApp::update`](crate::IcedApp::update) applies back to the runtime
//! state. This is the Iced counterpart of the egui backend's collected
//! `PendingInteraction` vec — but there it exists only because egui borrows
//! the data model for the whole frame; here `view` and `update` never overlap,
//! so the message stream *is* the interaction bridge. No `EditBuffers`, no
//! collect-then-apply step.
//!
//! `path` fields are absolute JSON Pointers (bindings are absolute per the
//! A2UI convention; template children resolve their relative path to absolute
//! before emitting a message), matching `DataModel::set`'s contract.

use serde_json::Value;

/// One user interaction, produced by a widget in `view` and handled in
/// `update`.
#[derive(Debug, Clone)]
pub enum Message {
    /// A Button was pressed — dispatch `Enter` to its component via core
    /// [`crate::dispatch_event`] + [`crate::apply_event_result`], exactly like
    /// the egui/Slint hosts' `handle_activate`. Carries the component id.
    ButtonActivate { component_id: String },
    /// A data-model write from an interactive widget (TextField / Slider /
    /// CheckBox / ChoicePicker). `path` is absolute.
    DataUpdate { path: String, value: Value },
    /// A Modal's `trigger` was activated — open that Modal locally.
    ModalTrigger { modal_id: String },
    /// A Modal's open panel was dismissed (overlay backdrop / close button) —
    /// close it.
    ModalClose { modal_id: String },
    /// The sample-browser sidebar: switch to sample `idx`.
    SelectSample(usize),
}
