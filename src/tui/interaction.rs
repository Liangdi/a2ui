//! Reusable keyboard-interaction helpers for A2UI TUI applications.
//!
//! The gallery app (`a2ui::gallery::app`) was the first place a complete,
//! validated event-dispatch pipeline was implemented. Several example programs
//! duplicate that ~40-line dispatch+apply boilerplate (and a couple carry bugs
//! where an `EventResult` is dropped). This module extracts the gallery's
//! semantics into small public functions so any app can replace its hand-rolled
//! copy with a single call to [`handle_key`] (or the granular pieces).
//!
//! The logic here mirrors `gallery::app::GalleryApp`'s
//! `dispatch_event_to_focused` / `process_event_result` methods exactly — it
//! does not introduce new behavior.

use crossterm::event::KeyCode;

use crate::core::catalog::Catalog;
use crate::core::event::{EventResult, InputEvent, InputKey};
use crate::core::message_processor::MessageProcessor;
use crate::core::model::component_context::ComponentContext;
use crate::tui::component_impl::ComponentRegistry;
use crate::tui::focus_manager::FocusManager;

/// Map a crossterm [`KeyCode`] to the framework-agnostic [`InputKey`].
///
/// Returns `None` for keys the A2UI model does not model (e.g. modifier-only
/// presses). Mirrors the `match` in `gallery::app::dispatch_event_to_focused`.
pub fn map_key_code(code: KeyCode) -> Option<InputKey> {
    let key = match code {
        KeyCode::Enter => InputKey::Enter,
        KeyCode::Tab => InputKey::Tab,
        KeyCode::BackTab => InputKey::BackTab,
        KeyCode::Up => InputKey::Up,
        KeyCode::Down => InputKey::Down,
        KeyCode::Left => InputKey::Left,
        KeyCode::Right => InputKey::Right,
        KeyCode::Backspace => InputKey::Backspace,
        KeyCode::Delete => InputKey::Delete,
        KeyCode::Esc => InputKey::Escape,
        KeyCode::Char(' ') => InputKey::Space,
        KeyCode::Char(c) => InputKey::Char(c),
        _ => return None,
    };
    Some(key)
}

/// Dispatch an already-built [`InputEvent`] to the focused component and return
/// whatever [`EventResult`] it produces.
///
/// This is `gallery::app::dispatch_event_to_focused` with the `KeyCode →
/// InputKey` mapping factored out (see [`map_key_code`]). It:
///
/// 1. Reads the focused component id from `focus`; returns `None` if nothing is
///    focused.
/// 2. Takes the first surface from the processor's surface group; returns
///    `None` if there are no surfaces.
/// 3. Looks the focused id up in that surface's components model to find its
///    `component_type`; returns `None` if the id is unknown.
/// 4. Looks the component type up in the `registry`; returns `None` if the type
///    has no TUI implementation.
/// 5. Builds a [`ComponentContext`] (empty `base_path`, focused id set) and
///    calls [`TuiComponent::handle_event`](crate::tui::component_impl::TuiComponent::handle_event).
///
/// All borrows on the surface's `data_model` / `components` are dropped before
/// the function returns, so the returned [`EventResult`] is fully owned.
pub fn dispatch_to_focused(
    processor: &MessageProcessor,
    registry: &ComponentRegistry,
    catalog: &Catalog,
    focus: &FocusManager,
    event: &InputEvent,
) -> Option<EventResult> {
    // 1. Focused component id.
    let focused_id = focus.focused_id()?.to_string();

    // 2. First surface.
    let surface = processor.model.surfaces().next()?;

    // 3. Resolve the focused component's type (drop the borrow before returning).
    let surface_id = surface.id.clone();
    let (comp_type, has_component) = {
        let components = surface.components.borrow();
        match components.get(&focused_id) {
            Some(m) => (m.component_type.clone(), true),
            None => (String::new(), false),
        }
    };
    if !has_component {
        return None;
    }

    // 4. TUI implementation for this type.
    let tui_comp = registry.get(&comp_type)?;

    // 5. Build context and dispatch.
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();
    let catalog_functions = &catalog.functions;

    let ctx = ComponentContext::new(
        focused_id.clone(),
        surface_id,
        &data_model,
        &components,
        catalog_functions,
        "",
        Some(focused_id.clone()),
    );

    let result = tui_comp.handle_event(&ctx, event);

    // Drop borrows before returning so the caller is free to mutate the
    // processor (mirrors the gallery's explicit `drop(...)` calls).
    drop(components);
    drop(data_model);

    result
}

/// Apply an [`EventResult`] produced by a component to the processor's state.
///
/// Replicates `gallery::app::process_event_result`. Returns `Some(path)` only
/// for an [`EventResult::Action`] that expects a response — the path is where
/// the eventual server response value should be written — so the caller can
/// drive the action-response cycle. Every other variant returns `None`.
pub fn apply_event_result(
    processor: &mut MessageProcessor,
    result: EventResult,
) -> Option<String> {
    match result {
        EventResult::Action {
            want_response,
            response_path,
            // event_name / context are intentionally ignored, matching the gallery.
            ..
        } => {
            if want_response {
                let surface_id = processor
                    .model
                    .surfaces()
                    .next()
                    .map(|s| s.id.clone());
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

/// The one-call keyboard pipeline: map a [`KeyCode`], dispatch it to the
/// focused component, and apply the resulting [`EventResult`].
///
/// Equivalent to the gallery's `dispatch_event_to_focused` immediately
/// followed by `process_event_result`. Returns the action `response_path`
/// (if any) so the caller can send the action and await a response.
///
/// The sequential borrows compile cleanly: [`dispatch_to_focused`] takes
/// `&processor` and returns an owned [`EventResult`], ending the shared borrow
/// before [`apply_event_result`] takes `&mut processor`.
pub fn handle_key(
    processor: &mut MessageProcessor,
    registry: &ComponentRegistry,
    catalog: &Catalog,
    focus: &FocusManager,
    code: KeyCode,
) -> Option<String> {
    let key = map_key_code(code)?;
    let event = InputEvent::KeyPress { key };
    let result = dispatch_to_focused(processor, registry, catalog, focus, &event)?;
    apply_event_result(processor, result)
}
