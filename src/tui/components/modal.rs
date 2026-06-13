//! Modal component — renders either the trigger child or the content child.

use ratatui::{Frame, layout::Rect};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicBoolean;
use crate::tui::component_impl::TuiComponent;

/// Modal component implementation.
///
/// In TUI, rendering a true modal overlay is complex. This component renders
/// the `trigger` child when closed and the `content` child when open, switching
/// between them based on the `isOpen` property (a `DynamicBoolean`).
pub struct ModalComponent;

impl TuiComponent for ModalComponent {
    fn name(&self) -> &'static str {
        "Modal"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        // Check if modal is open.
        let is_open = comp_model
            .get_property::<DynamicBoolean>("isOpen")
            .map(|db| ctx.data_context.resolve_dynamic_boolean(&db))
            .unwrap_or(false);

        if is_open {
            // Render content child.
            if let Some(content_id) = comp_model.get_property::<String>("content") {
                if area.width > 0 && area.height > 0 {
                    render_child(&content_id, area, frame, "");
                }
            }
        } else {
            // Render trigger child.
            if let Some(trigger_id) = comp_model.get_property::<String>("trigger") {
                if area.width > 0 && area.height > 0 {
                    render_child(&trigger_id, area, frame, "");
                }
            }
        }
    }
}
