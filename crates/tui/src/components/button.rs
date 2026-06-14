//! Button component — renders a clickable button with variant styling.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
};

use a2ui_base::model::component_context::ComponentContext;
use crate::component_impl::TuiComponent;

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

        // Determine variant.
        let variant: Option<String> = comp_model.get_property("variant");

        // Determine if this button has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());

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

        // If checks fail, apply DIM modifier. If focused, add REVERSED highlight.
        let final_style = if !checks_pass {
            base_style.add_modifier(Modifier::DIM)
        } else if is_focused {
            base_style.add_modifier(Modifier::REVERSED)
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
                render_child(&child_id, child_area, frame, "");
            }
        } else if let Some(a11y) = comp_model.accessibility() {
            // When no child is present, use the accessibility label as visible text.
            let a11y_text = a11y.label
                .as_ref()
                .map(|ds| ctx.data_context.resolve_dynamic_string(ds))
                .unwrap_or_default();
            if !a11y_text.is_empty() && child_area.width > 0 && child_area.height > 0 {
                let text = ratatui::text::Line::from(a11y_text);
                let widget = ratatui::widgets::Paragraph::new(text).style(final_style);
                frame.render_widget(widget, child_area);
            }
        }
    }

    fn natural_height(
        &self,
        ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // The render does `inner = area.shrink(1)` (2-cell margin). The default
        // variant additionally draws `Block::bordered()` (2-cell border), so its
        // single content line needs area.height - 4 >= 1 → 5. The primary /
        // borderless variants use `Borders::NONE`, so they only need margin → 3.
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return Some(3),
        };
        let variant: Option<String> = comp_model.get_property("variant");
        match variant.as_deref() {
            Some("primary") | Some("borderless") => Some(3),
            _ => Some(5),
        }
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &a2ui_base::event::InputEvent,
    ) -> Option<a2ui_base::event::EventResult> {
        a2ui_base::components::button::handle_event(ctx, event)
    }
}

/// Evaluate all `checks` on the component. Returns `true` if all pass (or none exist).
fn evaluate_checks(ctx: &ComponentContext, comp_model: &a2ui_base::model::component_model::ComponentModel) -> bool {
    match comp_model.checks() {
        Some(checks) => checks.iter().all(|rule| {
            ctx.data_context.resolve_dynamic_boolean_condition(&rule.condition)
        }),
        None => true,
    }
}
