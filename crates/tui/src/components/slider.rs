//! Slider component — renders a progress-bar style slider.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::component_impl::TuiComponent;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::{DynamicNumber, DynamicString};

/// Slider component implementation.
///
/// Renders a progress-bar style slider: `[=====>      ] 50`.
/// Uses `━` for the filled portion and `─` for the unfilled portion.
/// Applies a default 1-cell margin.
pub struct SliderComponent;

impl TuiComponent for SliderComponent {
    fn name(&self) -> &'static str {
        "Slider"
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

        // Resolve label.
        let label = match comp_model.get_property::<DynamicString>("label") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve value, min, max.
        let value = match comp_model.get_property::<DynamicNumber>("value") {
            Some(dn) => ctx.data_context.resolve_dynamic_number(&dn),
            None => 0.0,
        };
        let min = comp_model
            .get_property::<DynamicNumber>("min")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
            .unwrap_or(0.0);
        let max = comp_model
            .get_property::<DynamicNumber>("max")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
            .unwrap_or(100.0);

        // Calculate fill ratio.
        let range = max - min;
        let ratio = if range.abs() < f64::EPSILON {
            0.0
        } else {
            ((value - min) / range).clamp(0.0, 1.0)
        };

        // Build the slider bar string.
        // Reserve space for: label + space + brackets + value text
        let value_text = format!("{:.0}", value);
        // Total available width for the bar inside brackets
        let label_width = if label.is_empty() { 0 } else { label.len() + 1 };
        let value_width = value_text.len() + 1; // space before value
        let overhead = 2 + label_width + value_width; // [ ]
        let bar_width = (inner.width as usize).saturating_sub(overhead);

        let filled = (bar_width as f64 * ratio).round() as usize;
        let unfilled = bar_width.saturating_sub(filled);

        // Determine if this slider has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());
        let bar_color = if is_focused {
            Color::Yellow
        } else {
            Color::Cyan
        };

        // Resolve steps for discrete step markers.
        let steps = comp_model
            .get_property::<DynamicNumber>("steps")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn) as usize);

        let bar_str = if bar_width > 0 {
            if let Some(step_count) = steps {
                if step_count > 0 && step_count <= bar_width {
                    // Draw slider with step markers
                    let mut bar: Vec<char> = vec!['─'; bar_width];
                    for i in 0..=step_count {
                        let pos =
                            (bar_width as f64 * i as f64 / step_count as f64).round() as usize;
                        if pos < bar_width {
                            bar[pos] = '┬';
                        }
                    }
                    // Fill portion
                    for j in 0..filled {
                        if j < bar.len() {
                            bar[j] = '━';
                        }
                    }
                    format!("[{}]", bar.into_iter().collect::<String>())
                } else {
                    let filled_str: String = "━".repeat(filled);
                    let unfilled_str: String = "─".repeat(unfilled);
                    format!("[{}{}]", filled_str, unfilled_str)
                }
            } else {
                let filled_str: String = "━".repeat(filled);
                let unfilled_str: String = "─".repeat(unfilled);
                format!("[{}{}]", filled_str, unfilled_str)
            }
        } else {
            String::new()
        };

        // Build the display line.
        let mut spans = Vec::new();
        if !label.is_empty() {
            spans.push(Span::styled(
                format!("{} ", label),
                Style::default().fg(Color::White),
            ));
        }
        spans.push(Span::styled(bar_str, Style::default().fg(bar_color)));
        spans.push(Span::styled(
            format!(" {}", value_text),
            Style::default().fg(Color::White),
        ));

        let paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(paragraph, inner);
    }

    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // Single-line slider bar + 2 margin.
        Some(3)
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &a2ui_base::event::InputEvent,
    ) -> Option<a2ui_base::event::EventResult> {
        a2ui_base::components::slider::handle_event(ctx, event)
    }
}
