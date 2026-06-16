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
    /// A remote `Image` finished downloading — cache its decoded handle and
    /// re-render. Emitted by the fetch task spawned on sample load/switch.
    ImageLoaded {
        url: String,
        handle: iced::widget::image::Handle,
    },
    /// A remote `Image` failed to download — record the attempt so the fetch
    /// isn't retried on every view (the placeholder stays).
    ImageLoadFailed { url: String },
    /// A Modal's `trigger` was activated — open that Modal locally.
    ModalTrigger { modal_id: String },
    /// A Tabs title was clicked. When the Tabs' `activeTab` is a data binding
    /// the click instead emits a [`Message::DataUpdate`] (the model is the
    /// source of truth); this message is only used when `activeTab` is absent
    /// or literal — the gallery's samples fall in this case — so the selected
    /// tab is tracked locally (see `IcedApp::local_tabs`).
    TabActivate { component_id: String, index: usize },
    /// A Modal's open panel was dismissed (overlay backdrop / close button) —
    /// close it.
    ModalClose { modal_id: String },
    /// The sample-browser sidebar: switch to sample `idx`.
    SelectSample(usize),
}
