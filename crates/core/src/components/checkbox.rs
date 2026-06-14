//! CheckBox component behavior — framework-agnostic `handle_event`.

use crate::event::{EventResult, InputEvent, InputKey};
use crate::model::component_context::ComponentContext;
use crate::protocol::common_types::DynamicBoolean;

/// Handle a key-press for a CheckBox (toggles its bound boolean on Enter/Space).
pub fn handle_event(ctx: &ComponentContext, event: &InputEvent) -> Option<EventResult> {
    let InputEvent::KeyPress { key } = event;
    if !matches!(key, InputKey::Enter | InputKey::Space) {
        return None;
    }

    let comp_model = ctx.components.get(&ctx.component_id)?;

    // Get the value binding to find the data path.
    let value = comp_model.get_property::<DynamicBoolean>("value")?;
    if let DynamicBoolean::Binding(binding) = value {
        return Some(EventResult::Toggle { path: binding.path });
    }
    None
}
