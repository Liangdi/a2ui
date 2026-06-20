//! Framework-agnostic interaction helpers — applying component interaction
//! results to the runtime state.
//!
//! This module holds the parts of the keyboard/interaction pipeline that touch
//! only core types, so every backend (ratatui, Slint, …) shares them. The
//! key-mapping and component-dispatch halves stay backend-side: each backend
//! maps its own key enum to [`crate::event::InputKey`] and dispatches via its
//! own component registry (or, after the shared-behavior extraction, via
//! [`crate::components::dispatch_event`]).
//!
//! See the tui crate's `interaction` module for the original implementation
//! this was lifted from.

use crate::event::EventResult;
use crate::message_processor::MessageProcessor;

/// Apply an [`EventResult`] produced by a component to the processor's state.
///
/// Returns `Some(path)` only for an [`EventResult::Action`] that expects a
/// response — the path is where the eventual server response value should be
/// written — so the caller can drive the action-response cycle. Every other
/// variant returns `None`.
pub fn apply_event_result(processor: &mut MessageProcessor, result: EventResult) -> Option<String> {
    match result {
        EventResult::Action {
            want_response,
            response_path,
            // event_name / context are intentionally ignored, matching the gallery.
            ..
        } => {
            if want_response {
                let surface_id = processor.model.surfaces().next().map(|s| s.id.clone());
                if let Some(sid) = surface_id {
                    let action_id = uuid::Uuid::new_v4().to_string();
                    let _ = processor.register_action(&sid, &action_id, response_path.clone());
                }
                response_path
            } else {
                None
            }
        }
        EventResult::DataUpdate { path, value } => {
            if let Some(surface) = processor.model.surfaces_mut().next() {
                surface.data_model.borrow_mut().set(&path, value);
            }
            None
        }
        EventResult::Toggle { path } => {
            if let Some(surface) = processor.model.surfaces_mut().next() {
                let current = surface
                    .data_model
                    .borrow()
                    .get(&path)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                surface
                    .data_model
                    .borrow_mut()
                    .set(&path, serde_json::json!(!current));
            }
            None
        }
        EventResult::Consumed => None,
    }
}
