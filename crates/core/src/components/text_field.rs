//! TextField component behavior — framework-agnostic `handle_event`.

use crate::event::{EventResult, InputEvent, InputKey};
use crate::model::component_context::ComponentContext;
use crate::protocol::common_types::DynamicString;

/// Handle a key-press for a TextField (appends chars / backspaces the bound string).
pub fn handle_event(ctx: &ComponentContext, event: &InputEvent) -> Option<EventResult> {
    let comp_model = ctx.components.get(&ctx.component_id)?;

    // Get the value binding path.
    let value_ds = comp_model.get_property::<DynamicString>("value")?;
    let binding = match value_ds {
        DynamicString::Binding(b) => b,
        _ => return None,
    };

    let current =
        ctx.data_context
            .resolve_dynamic_string(&DynamicString::Binding(binding.clone()));

    match event {
        InputEvent::KeyPress { key: InputKey::Char(c) } => {
            let new_value = format!("{}{}", current, c);
            Some(EventResult::DataUpdate {
                path: binding.path.clone(),
                value: serde_json::Value::String(new_value),
            })
        }
        InputEvent::KeyPress {
            key: InputKey::Backspace,
        } => {
            let new_value = if let Some((idx, _)) = current.char_indices().next_back() {
                &current[..idx]
            } else {
                ""
            }
            .to_string();
            Some(EventResult::DataUpdate {
                path: binding.path.clone(),
                value: serde_json::Value::String(new_value),
            })
        }
        _ => None,
    }
}
