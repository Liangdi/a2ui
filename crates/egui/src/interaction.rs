//! Collected-then-applied interaction bridge.
//!
//! The recursive walker ([`crate::walker::render_node`]) can't mutate the
//! [`MessageProcessor`] while the surface's `data_model` / `components` are
//! borrowed for the duration of a frame's walk. So each egui widget that the
//! user interacts with pushes a [`PendingInteraction`] into a `Vec` carried
//! through the walk, and [`EguiApp::apply_pending`] consumes the vec *after*
//! the walk — once the borrows are dropped.
//!
//! This mirrors the drop-borrows-then-mutate pattern in the ratatui gallery
//! (`dispatch_event_to_focused` drops its borrows before `process_event_result`)
//! and the callback-then-redraw pattern in the Slint host (`handle_activate`
//! scopes its borrow block, then mutates).

use serde_json::Value;

/// One deferred interaction, collected during a frame's walk and applied after.
#[derive(Debug, Clone)]
pub enum PendingInteraction {
    /// A Button was clicked — dispatch `Enter` to its component via core
    /// [`crate::dispatch_event`] + [`crate::apply_event_result`], exactly like
    /// the Slint host's `handle_activate`. Carries the component id.
    ButtonActivate { component_id: String },
    /// A data-model write from an interactive widget (TextField/Slider/…).
    /// `path` is an **absolute** JSON Pointer (bindings are absolute per the
    /// A2UI convention; template children resolve their relative path to
    /// absolute before emitting this). Matches `DataModel::set`'s contract.
    DataUpdate { path: String, value: Value },
    /// Toggle a boolean at `path` (CheckBox without a direct target value).
    /// `path` is absolute.
    Toggle { path: String },
    /// A Modal's `trigger` was activated — open that Modal locally.
    ModalTrigger { modal_id: String },
    /// A Modal's open panel was dismissed (backdrop / close button) — close it.
    ModalClose { modal_id: String },
}
