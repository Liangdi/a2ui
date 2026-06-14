//! Framework-agnostic component **behavior** — the `handle_event` logic for
//! the interactive component types whose handlers touch only core types.
//!
//! Rendering is backend-specific (ratatui paints a `Frame`, Slint builds a
//! reactive tree), but the *semantics* of how a component reacts to a key press
//! (fire an action, write a data-model value, toggle, …) are identical across
//! backends. Extracting that logic here lets every backend reuse one
//! implementation instead of duplicating it.
//!
//! Each submodule exposes
//! `pub fn handle_event(ctx: &ComponentContext, event: &InputEvent) -> Option<EventResult>`.
//! [`dispatch_event`] routes by component-type name.
//!
//! ## Scope
//!
//! Only handlers with no backend coupling are extracted here:
//! [`button`], [`checkbox`], [`slider`], [`text_field`]. The remaining
//! interactive types stay backend-specific for now:
//! - `choice_picker` / `tabs` — their handlers share locally-defined
//!   deserializable types (`ChoiceOption`, `TabEntry`) with the ratatui render
//!   path; extracting them would pull render types into core.
//! - `date_time_input` — its handler shares `chrono` parsing helpers with render.
//! - `audio_player` — drives real audio hardware (rodio), inherently backend-bound.
//!
//! A Slint backend reuses the four shared handlers via [`dispatch_event`] and
//! reimplements the rest locally until they are promoted here.

use crate::event::{EventResult, InputEvent};
use crate::model::component_context::ComponentContext;

pub mod button;
pub mod checkbox;
pub mod slider;
pub mod text_field;

/// Route a key-press event to the named component type's [`handle_event`].
///
/// Returns `None` for types without a shared handler (unknown types,
/// non-interactive types like Text/Column, or the backend-specific interactive
/// types listed in the module docs). Callers build a [`ComponentContext`] for
/// the target component, then call this.
pub fn dispatch_event(
    comp_type: &str,
    ctx: &ComponentContext,
    event: &InputEvent,
) -> Option<EventResult> {
    match comp_type {
        "Button" => button::handle_event(ctx, event),
        "CheckBox" => checkbox::handle_event(ctx, event),
        "Slider" => slider::handle_event(ctx, event),
        "TextField" => text_field::handle_event(ctx, event),
        _ => None,
    }
}
