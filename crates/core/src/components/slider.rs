//! Slider component behavior — framework-agnostic `handle_event`.

use crate::event::{EventResult, InputEvent, InputKey};
use crate::model::component_context::ComponentContext;
use crate::protocol::common_types::DynamicNumber;

/// Handle a key-press for a Slider (steps the bound number Left/Right).
pub fn handle_event(ctx: &ComponentContext, event: &InputEvent) -> Option<EventResult> {
    let comp_model = ctx.components.get(&ctx.component_id)?;

    let value_dn = comp_model.get_property::<DynamicNumber>("value")?;
    let binding = match value_dn {
        DynamicNumber::Binding(b) => b,
        _ => return None,
    };

    let current =
        ctx.data_context
            .resolve_dynamic_number(&DynamicNumber::Binding(binding.clone()));
    let min = comp_model
        .get_property::<DynamicNumber>("min")
        .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
        .unwrap_or(0.0);
    let max = comp_model
        .get_property::<DynamicNumber>("max")
        .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
        .unwrap_or(100.0);

    let steps = comp_model
        .get_property::<DynamicNumber>("steps")
        .map(|dn| ctx.data_context.resolve_dynamic_number(&dn) as usize)
        .unwrap_or(10);
    let step = if steps > 0 {
        (max - min) / steps as f64
    } else {
        1.0
    };

    let delta = match event {
        InputEvent::KeyPress {
            key: InputKey::Right,
        } => step,
        InputEvent::KeyPress { key: InputKey::Left } => -step,
        _ => return None,
    };

    let new_value = (current + delta).clamp(min, max);
    Some(EventResult::DataUpdate {
        path: binding.path.clone(),
        value: serde_json::json!(new_value),
    })
}
