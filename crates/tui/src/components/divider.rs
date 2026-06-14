//! Divider component — renders a horizontal or vertical separator line.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::Paragraph,
};

use a2ui_base::model::component_context::ComponentContext;
use crate::component_impl::TuiComponent;

/// Divider component implementation.
///
/// Renders a horizontal `─────` line or a vertical `│` character
/// in a dim color to visually separate content.
/// Applies a default 1-cell margin.
pub struct DividerComponent;

impl TuiComponent for DividerComponent {
    fn name(&self) -> &'static str {
        "Divider"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
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

        let axis: Option<String> = comp_model.get_property("axis");
        let dim_style = Style::default().fg(Color::DarkGray);

        match axis.as_deref() {
            Some("vertical") => {
                // Vertical divider: render a column of '│' characters.
                let line: String = "│".repeat(inner.height as usize);
                let paragraph = Paragraph::new(line).style(dim_style);
                frame.render_widget(paragraph, inner);
            }
            _ => {
                // Horizontal divider (default): render a line of '─' characters.
                let line: String = "─".repeat(inner.width as usize);
                let paragraph = Paragraph::new(line).style(dim_style);
                frame.render_widget(paragraph, inner);
            }
        }
    }

    fn natural_height(
        &self,
        ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // Read the axis property: a vertical divider fills its column (no
        // intrinsic height); a horizontal divider is a single line + 2-cell
        // margin.
        let comp_model = ctx.components.get(&ctx.component_id)?;
        let axis: Option<String> = comp_model.get_property("axis");
        match axis.as_deref() {
            Some("vertical") => None,
            _ => Some(3),
        }
    }
}
