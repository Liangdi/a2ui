//! Button component — renders a clickable button with variant styling.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
};

use crate::core::model::component_context::ComponentContext;
use crate::tui::component_impl::TuiComponent;

/// Button component implementation.
///
/// Renders a bordered or styled block with optional child content inside.
/// Supports variants: "primary", "borderless", "default".
/// If `checks` conditions resolve to false, the button is dimmed.
/// Applies a default 1-cell margin.
pub struct ButtonComponent;

impl TuiComponent for ButtonComponent {
    fn name(&self) -> &'static str {
        "Button"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame),
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

        // Determine variant.
        let variant: Option<String> = comp_model.get_property("variant");

        // Evaluate checks — if any condition resolves to false, dim the button.
        let checks_pass = evaluate_checks(ctx, comp_model);

        // Build the block and style based on variant.
        let (block, base_style) = match variant.as_deref() {
            Some("primary") => {
                let style = Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD);
                let block = Block::default().style(style).borders(Borders::NONE);
                (block, style)
            }
            Some("borderless") => {
                let style = Style::default().add_modifier(Modifier::UNDERLINED);
                let block = Block::default().style(style).borders(Borders::NONE);
                (block, style)
            }
            _ => {
                // "default" variant: plain bordered block.
                let style = Style::default();
                let block = Block::default().style(style).borders(Borders::ALL);
                (block, style)
            }
        };

        // If checks fail, apply DIM modifier.
        let final_style = if !checks_pass {
            base_style.add_modifier(Modifier::DIM)
        } else {
            base_style
        };

        let block = block.style(final_style);

        // Compute the inner area for the child (inside the block's borders).
        let child_area = block.inner(inner);

        // Render the block itself.
        frame.render_widget(block, inner);

        // If the button has a child, render it inside the block's inner area.
        if let Some(child_id) = comp_model.child() {
            if child_area.width > 0 && child_area.height > 0 {
                render_child(&child_id, child_area, frame);
            }
        }
    }
}

/// Evaluate all `checks` on the component. Returns `true` if all pass (or none exist).
fn evaluate_checks(ctx: &ComponentContext, comp_model: &crate::core::model::component_model::ComponentModel) -> bool {
    match comp_model.checks() {
        Some(checks) => checks.iter().all(|rule| {
            ctx.data_context.resolve_dynamic_boolean_condition(&rule.condition)
        }),
        None => true,
    }
}
