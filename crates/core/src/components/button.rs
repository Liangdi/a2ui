//! Button component behavior — framework-agnostic `handle_event`.

use std::collections::HashMap;

use crate::event::{EventResult, InputEvent, InputKey};
use crate::model::component_context::ComponentContext;
use crate::protocol::common_types::{Action, DynamicValue};

/// Handle a key-press for a Button (fires its `action` on Enter).
pub fn handle_event(ctx: &ComponentContext, event: &InputEvent) -> Option<EventResult> {
    let InputEvent::KeyPress { key } = event;
    if *key != InputKey::Enter {
        return None;
    }

    let comp_model = ctx.components.get(&ctx.component_id)?;
    let action = comp_model.action()?;

    match action {
        Action::Event { event: action_event } => {
            let mut context = HashMap::new();
            for (k, dv) in &action_event.context {
                context.insert(k.clone(), ctx.data_context.resolve_dynamic_value(&dv));
            }
            Some(EventResult::Action {
                event_name: action_event.name.clone(),
                context,
                want_response: action_event.want_response,
                response_path: action_event.response_path.clone(),
            })
        }
        Action::FunctionCall { function_call: fc } => {
            // Execute local function call.
            let _result = ctx
                .data_context
                .resolve_dynamic_value(&DynamicValue::Function(fc));
            Some(EventResult::Consumed)
        }
    }
}
