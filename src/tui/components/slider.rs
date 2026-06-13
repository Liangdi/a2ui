//! Slider component — renders a progress-bar style slider.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::core::event::{EventResult, InputEvent, InputKey};
use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::{DynamicNumber, DynamicString};
use crate::tui::component_impl::TuiComponent;

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
        let bar_width = if (inner.width as usize) > overhead {
            inner.width as usize - overhead
        } else {
            0
        };

        let filled = (bar_width as f64 * ratio).round() as usize;
        let unfilled = bar_width.saturating_sub(filled);

        // Determine if this slider has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());
        let bar_color = if is_focused { Color::Yellow } else { Color::Cyan };

        let bar_str = if bar_width > 0 {
            let filled_str: String = "━".repeat(filled);
            let unfilled_str: String = "─".repeat(unfilled);
            format!("[{}{}]", filled_str, unfilled_str)
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
        spans.push(Span::styled(
            bar_str,
            Style::default().fg(bar_color),
        ));
        spans.push(Span::styled(
            format!(" {}", value_text),
            Style::default().fg(Color::White),
        ));

        let paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(paragraph, inner);
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &crate::core::event::InputEvent,
    ) -> Option<crate::core::event::EventResult> {
        let comp_model = ctx.components.get(&ctx.component_id)?;

        let value_dn = comp_model.get_property::<DynamicNumber>("value")?;
        let binding = match value_dn {
            DynamicNumber::Binding(b) => b,
            _ => return None,
        };

        let current = ctx.data_context.resolve_dynamic_number(
            &DynamicNumber::Binding(binding.clone()),
        );
        let min = comp_model
            .get_property::<DynamicNumber>("min")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
            .unwrap_or(0.0);
        let max = comp_model
            .get_property::<DynamicNumber>("max")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
            .unwrap_or(100.0);

        let steps = comp_model
            .get_property::<DynamicNumber>("steps")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn) as usize)
            .unwrap_or(10);
        let step = if steps > 0 {
            (max - min) / steps as f64
        } else {
            1.0
        };

        let delta = match event {
            InputEvent::KeyPress { key: InputKey::Right } => step,
            InputEvent::KeyPress { key: InputKey::Left } => -step,
            _ => return None,
        };

        let new_value = (current + delta).clamp(min, max);
        Some(EventResult::DataUpdate {
            path: binding.path.clone(),
            value: serde_json::json!(new_value),
        })
    }
}
