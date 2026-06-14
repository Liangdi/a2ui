//! Card component — renders a rounded-border container with a single child.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders},
};

use a2ui_core::model::component_context::ComponentContext;
use crate::component_impl::TuiComponent;

/// Card component implementation.
///
/// Renders a `Block` with rounded borders and padding, then renders the
/// single child inside the block's inner area.
/// Applies a default 1-cell margin.
pub struct CardComponent;

impl TuiComponent for CardComponent {
    fn name(&self) -> &'static str {
        "Card"
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

        // Apply default 1-cell margin on all sides (never collapses to zero).
        let inner = crate::layout_engine::padded_content(area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Build the card block with rounded borders and a subtle style.
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default());

        // Compute the inner content area (inside the block borders).
        let child_area = block.inner(inner);

        // Render the block itself.
        frame.render_widget(block, inner);

        // Render the single child inside the card, if present.
        if let Some(child_id) = comp_model.child() {
            if child_area.width > 0 && child_area.height > 0 {
                render_child(&child_id, child_area, frame, "");
            }
        }
    }

    /// Natural height = child's natural height + chrome (margin 2 + border 2 = 4).
    fn natural_height(
        &self,
        ctx: &ComponentContext,
        available_width: u16,
        measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id)?;
        let child_id = comp_model.child()?;
        // The child renders inside margin(2) + border(2); give it the reduced width.
        let inner_width = available_width.saturating_sub(4);
        let child_h = measure_child(&child_id, "", inner_width)?;
        Some(child_h.saturating_add(4))
    }
}
