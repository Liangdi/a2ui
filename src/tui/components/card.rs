//! Card component — renders a rounded-border container with a single child.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders},
};

use crate::core::model::component_context::ComponentContext;
use crate::tui::component_impl::TuiComponent;

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
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        // Apply default 1-cell margin on all sides.
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };

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
}
