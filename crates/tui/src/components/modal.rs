//! Modal component — always renders its trigger in-place; the open content is
//! shown as a floating overlay by the surface renderer (a real modal dialog),
//! not by swapping in here.

use ratatui::{Frame, layout::Rect};

use a2ui_base::model::component_context::ComponentContext;
use crate::component_impl::TuiComponent;

/// Modal component implementation.
///
/// Always renders the `trigger` child in its layout position. When open, the
/// `content` child is drawn as a centered overlay on top of the whole surface
/// by [`crate::surface::SurfaceRenderer`] — so the trigger keeps its place
/// (and focus) and the dialog actually floats above the UI instead of
/// replacing the trigger inline.
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
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        // The trigger always renders in-place; the open content is overlaid by
        // the surface renderer, not here.
        if let Some(trigger_id) = comp_model.get_property::<String>("trigger") {
            if area.width > 0 && area.height > 0 {
                render_child(&trigger_id, area, frame, "");
            }
        }
    }

    fn natural_height(
        &self,
        ctx: &ComponentContext,
        available_width: u16,
        measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id)?;
        // Sizing follows the trigger (the in-place element); the overlay content
        // is sized independently by the surface renderer.
        let child_id = comp_model.get_property::<String>("trigger")?;
        measure_child(&child_id, "", available_width)
    }
}
