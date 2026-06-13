//! Modal component — renders the trigger child (modal overlay is complex in TUI).

use ratatui::{Frame, layout::Rect};

use crate::core::model::component_context::ComponentContext;
use crate::tui::component_impl::TuiComponent;

/// Modal component implementation.
///
/// In TUI, rendering a true modal overlay is complex. This component simply
/// renders the `trigger` child normally. Full modal overlay support would
/// require application-level state management.
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

        // Render the trigger child normally.
        if let Some(trigger_id) = comp_model.get_property::<String>("trigger") {
            if area.width > 0 && area.height > 0 {
                render_child(&trigger_id, area, frame, "");
            }
        }
    }
}
