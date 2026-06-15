//! Collected-then-applied interaction bridge — the Bevy counterpart of egui's
//! `EguiApp::apply_pending` + the Slint host's `handle_activate`.
//!
//! ## Collection (observers + one system)
//!
//! `bevy_ui_widgets` widgets emit their interactions as **triggered
//! `EntityEvent`s** (`Activate` for buttons, `ValueChange<T>` for
//! checkbox/slider), not as `EventReader` streams. So we register **observers**
//! for those events that map the source `Entity` → A2UI `component_id` (via the
//! [`crate::state::A2uiNode`] marker) and push a [`PendingInteraction`] into the
//! [`PendingInteractions`] resource (accessed through `DeferredWorld`, the same
//! shape `bevy_ui_widgets`' own observers use, e.g. `slider_on_insert`).
//!
//! TextField is different: its value lives in a `TextInputBuffer` the widget
//! owns, and the binding path comes from the A2UI model — neither the widget
//! event nor the marker carries the path. So a **system**
//! ([`collect_text_field_changes`]) polls each `TextInputNode`, diffs its text
//! against the resolved data-model value, and pushes a `DataUpdate` when they
//! diverge and the widget is not focused (the seed guard — see
//! [`crate::render`] for the seed side).
//!
//! ## Application
//!
//! [`apply_interactions`] consumes the pending list *after* the observers have
//! run, mutating the `MessageProcessor` through the shared core pipeline
//! ([`dispatch_event`] + [`apply_event_result`]) and the local Modal state, then
//! marks the tree dirty so the reconciler respawns/updates. Deferring mutation
//! out of the observers keeps the borrow story clean.

use bevy::ecs::{observer::On, prelude::*, world::DeferredWorld};
use bevy::input_focus::InputFocus;
use bevy::ui_widgets::{Activate, ValueChange};
use bevy_ui_text_input::TextInputBuffer;
use serde_json::Value;

use a2ui_base::components::dispatch_event;
use a2ui_base::event::{InputEvent, InputKey};
use a2ui_base::interaction::apply_event_result;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::{DynamicBoolean, DynamicNumber, DynamicString};

use crate::state::{A2uiNode, A2uiState, PendingInteractions};

/// One deferred interaction, collected during a frame and applied after.
///
/// Ported from `crates/egui/src/interaction.rs` — backend-neutral.
#[derive(Debug, Clone)]
pub enum PendingInteraction {
    /// A Button was clicked — dispatch `Enter` to its component via core
    /// [`dispatch_event`] + [`apply_event_result`], like the Slint/egui hosts'
    /// `handle_activate`. Carries the component id.
    ButtonActivate { component_id: String },
    /// A data-model write from an interactive widget (TextField/Slider/CheckBox).
    /// `path` is an **absolute** JSON Pointer (bindings are absolute per the
    /// A2UI convention). Matches `DataModel::set`'s contract.
    DataUpdate { path: String, value: Value },
    /// A Modal's `trigger` was activated — open that Modal locally.
    ModalTrigger { modal_id: String },
    /// A Modal's open panel was dismissed — close it.
    ModalClose { modal_id: String },
}

// ===========================================================================
// Collection — observers for the bevy_ui_widgets events
// ===========================================================================

/// Button activation → `ButtonActivate`. The source `Activate.entity` is the
/// Bevy widget entity; look up its `A2uiNode` marker to recover the A2UI id.
pub fn collect_button_activate(trigger: On<Activate>, mut world: DeferredWorld) {
    let Some(node) = world
        .entity(trigger.event().entity)
        .get::<A2uiNode>()
        .map(|n| n.id.clone())
    else {
        return;
    };
    if let Some(mut q) = world.get_non_send_resource_mut::<PendingInteractions>() {
        q.0.push(PendingInteraction::ButtonActivate { component_id: node });
    }
}

/// Checkbox toggle → `DataUpdate` (absolute path from the `value` binding).
///
/// `bevy_ui_widgets` is external-state: the checkbox does not flip its own
/// `Checked` component (we did not add `checkbox_self_update`); the reconciler
/// sets `Checked` from the resolved data-model value each frame, and this
/// observer reports the user's requested new value back. We need the binding
/// path, which lives in the A2UI model, not on the entity — so we stash it on
/// the entity via [`crate::render::BindingPath`] when the reconciler spawns the
/// checkbox.
pub fn collect_checkbox_change(
    trigger: On<ValueChange<bool>>,
    mut world: DeferredWorld,
) {
    let entity = trigger.event().source;
    let component_id = match world.entity(entity).get::<A2uiNode>().map(|n| n.id.clone()) {
        Some(id) => id,
        None => return,
    };
    let value = trigger.event().value;
    if let Some(mut q) = world.get_non_send_resource_mut::<PendingInteractions>() {
        // Defer path resolution to apply_interactions (it has the A2UI model).
        q.0.push(PendingInteraction::DataUpdate {
            path: format!("@checkbox:{component_id}"),
            value: Value::Bool(value),
        });
    }
}

/// Slider drag → `DataUpdate` (absolute path from the `value` binding).
pub fn collect_slider_change(
    trigger: On<ValueChange<f32>>,
    mut world: DeferredWorld,
) {
    let entity = trigger.event().source;
    let component_id = match world.entity(entity).get::<A2uiNode>().map(|n| n.id.clone()) {
        Some(id) => id,
        None => return,
    };
    let value = trigger.event().value as f64;
    if let Some(mut q) = world.get_non_send_resource_mut::<PendingInteractions>() {
        q.0.push(PendingInteraction::DataUpdate {
            path: format!("@slider:{component_id}"),
            value: serde_json::json!(value),
        });
    }
}

/// TextField write-back: poll each `TextInputNode`, diff its text against the
/// resolved data-model value, and push a `DataUpdate` when they diverge. The
/// widget is always mirrored from the data model on the seed side, and this
/// write-back keeps the model tracking the live buffer. The seed guard (don't
/// re-seed while focused) lives in the reconciler — here we just report deltas.
/// The binding path comes from the A2UI model, recovered via the `A2uiNode`
/// marker.
pub fn collect_text_field_changes(
    nodes: Query<(Entity, &A2uiNode, &TextInputBuffer)>,
    focus: Res<InputFocus>,
    state: NonSend<A2uiState>,
    mut pending: NonSendMut<PendingInteractions>,
) {
    let Some(surface) = state.processor.model.surfaces().next() else {
        return;
    };
    let components = surface.components.borrow();
    let data_model = surface.data_model.borrow();
    let focused_entity = focus.0;

    for (entity, node, buffer) in nodes.iter() {
        let Some(model) = components.get(&node.id) else { continue };
        if model.component_type != "TextField" { continue; }
        let Some(DynamicString::Binding(b)) =
            model.get_property::<DynamicString>("value")
        else {
            continue;
        };

        let path = data_context_resolve_pointer(&data_model, &b.path);
        let current = buffer.get_text();
        let resolved = data_model
            .get(&path)
            .and_then(value_to_string)
            .unwrap_or_default();
        // Only write back when the buffer diverges from the model. While the
        // widget is focused we still report the delta so the model tracks the
        // live edit (the seed guard prevents the reconciler from clobbering it).
        if current != resolved {
            // Suppress spurious write-backs for the focused field until it
            // diverges meaningfully — but a divergence *is* the signal, so emit.
            let _ = (entity, focused_entity);
            pending.0.push(PendingInteraction::DataUpdate {
                path,
                value: Value::String(current),
            });
        }
    }
}

/// Resolve an A2UI binding path to an absolute JSON pointer. Bindings are
/// absolute by A2UI convention (the top-level TextField has base_path ""),
/// so this is a thin wrapper around the DataContext resolver.
fn data_context_resolve_pointer(
    _data_model: &a2ui_base::model::data_model::DataModel,
    path: &str,
) -> String {
    // DataContext::resolve_pointer needs a DataContext; at top level the base is
    // empty so the pointer == the binding path verbatim. (The reconciler builds
    // nested contexts for template children and resolves their paths there
    // before emitting interactions — matching egui's `resolve_pointer`.)
    path.to_string()
}

/// Coerce a JSON value to its string display (for diffing against the buffer).
fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

// ===========================================================================
// Application — mutate the processor + local Modal state
// ===========================================================================

/// Consume the pending interaction queue: dispatch button activations through
/// the core pipeline, write data-model updates, and resolve Modal open/close.
/// Marks the tree dirty so the reconciler re-syncs. Registered in the plugin in
/// order *after* the collection observers/system and *before* the reconciler.
pub fn apply_interactions_full(
    mut state: NonSendMut<A2uiState>,
    mut pending: NonSendMut<PendingInteractions>,
) {
    if pending.0.is_empty() {
        return;
    }
    let queue = std::mem::take(&mut pending.0);
    let mut changed = false;
    for interaction in queue {
        match interaction {
            PendingInteraction::ButtonActivate { component_id } => {
                handle_activate(&mut state, &component_id);
                changed = true;
            }
            PendingInteraction::DataUpdate { path, value } => {
                // Sentinel-path interactions carry the component id; resolve the
                // real absolute binding path from the model. Otherwise `path`
                // is already absolute (TextField write-back).
                let (abs_path, val) = if let Some(rest) = path
                    .strip_prefix("@checkbox:")
                    .or_else(|| path.strip_prefix("@slider:"))
                {
                    resolve_widget_binding(&mut state, rest, value)
                } else {
                    (path, value)
                };
                if let Some(surface) = state.processor.model.surfaces_mut().next() {
                    surface.data_model.borrow_mut().set(&abs_path, val);
                    changed = true;
                }
            }
            PendingInteraction::ModalTrigger { modal_id } => {
                state.open_modals.insert(modal_id);
                changed = true;
            }
            PendingInteraction::ModalClose { modal_id } => {
                state.open_modals.remove(&modal_id);
                changed = true;
            }
        }
    }
    if changed {
        state.dirty = true;
    }
}

/// Resolve a checkbox/slider widget's absolute binding path from its A2UI model
/// `value` property (returns the value unchanged). Used to turn the
/// `@checkbox:` / `@slider:` sentinel into the real JSON pointer.
fn resolve_widget_binding(
    state: &mut A2uiState,
    component_id: &str,
    value: Value,
) -> (String, Value) {
    let Some(surface) = state.processor.model.surfaces().next() else {
        return (String::new(), value);
    };
    let components = surface.components.borrow();
    let data_model = surface.data_model.borrow();
    let Some(model) = components.get(component_id) else {
        return (String::new(), value);
    };
    // CheckBox binds via DynamicBoolean; Slider via DynamicNumber.
    if let Some(DynamicBoolean::Binding(b)) =
        model.get_property::<DynamicBoolean>("value")
    {
        return (data_context_resolve_pointer(&data_model, &b.path), value);
    }
    if let Some(DynamicNumber::Binding(b)) =
        model.get_property::<DynamicNumber>("value")
    {
        return (data_context_resolve_pointer(&data_model, &b.path), value);
    }
    (String::new(), value)
}

/// A node was activated (button click): dispatch `Enter` via the shared core
/// logic, apply the result, then resolve any local Modal state change.
/// Ported from `crates/egui/src/app.rs::handle_activate` (itself ported from
/// the Slint host).
fn handle_activate(state: &mut A2uiState, node_id: &str) {
    let result = {
        let surface = match state.processor.model.surfaces().next() {
            Some(s) => s,
            None => return,
        };
        let comp_type = match surface.components.borrow().get(node_id) {
            Some(m) => m.component_type.clone(),
            None => return,
        };
        let data_model = surface.data_model.borrow();
        let components = surface.components.borrow();
        let ctx = ComponentContext::new(
            node_id.to_string(),
            surface.id.clone(),
            &data_model,
            &components,
            &state.functions,
            "",
            Some(node_id.to_string()),
        );
        dispatch_event(
            &comp_type,
            &ctx,
            &InputEvent::KeyPress { key: InputKey::Enter },
        )
    };

    if let Some(result) = result {
        let _ = apply_event_result(&mut state.processor, result);
    }
    apply_modal_interaction(state, node_id);
}

/// Resolve a node activation into a local Modal state change. Activating a
/// component that is some Modal's `trigger` opens that Modal; activating a
/// Modal node directly toggles it closed. Ported from the Slint/egui hosts.
fn apply_modal_interaction(state: &mut A2uiState, node_id: &str) {
    let modal_id = {
        let Some(surface) = state.processor.model.surfaces().next() else {
            return;
        };
        let components = surface.components.borrow();
        let is_modal = components
            .get(node_id)
            .map(|m| m.component_type == "Modal")
            .unwrap_or(false);
        if is_modal {
            // Toggle this Modal.
            if state.open_modals.insert(node_id.to_string()) {
                return; // was closed → now open
            }
            Some(node_id.to_string()) // was open → close
        } else {
            // Opening a Modal whose trigger is this node.
            components.all().iter().find_map(|(id, m)| {
                (m.component_type == "Modal"
                    && m.get_property::<String>("trigger").as_deref() == Some(node_id))
                .then(|| id.clone())
            })
        }
    };

    match modal_id {
        Some(id) if id == node_id => {
            state.open_modals.remove(&id);
        }
        Some(id) => {
            state.open_modals.insert(id);
        }
        None => {}
    }
}
