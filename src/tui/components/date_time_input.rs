//! DateTimeInput component — renders a date/time input display.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicString;
use crate::tui::component_impl::TuiComponent;

/// DateTimeInput component implementation.
///
/// Renders a bordered block with a date/time icon and the ISO date string.
/// Display: `[📅 2026-06-13 14:30]` style.
/// Applies a default 1-cell margin.
pub struct DateTimeInputComponent;

impl TuiComponent for DateTimeInputComponent {
    fn name(&self) -> &'static str {
        "DateTimeInput"
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

        // Resolve value (ISO date string).
        let value = match comp_model.get_property::<DynamicString>("value") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve enableDate and enableTime flags.
        let enable_date: bool = comp_model.get_property("enableDate").unwrap_or(true);
        let enable_time: bool = comp_model.get_property("enableTime").unwrap_or(true);
        let _min = comp_model.get_property::<DynamicString>("min")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds));
        let _max = comp_model.get_property::<DynamicString>("max")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds));

        // Choose icon based on enabled modes.
        let icon = match (enable_date, enable_time) {
            (true, true) => "\u{1F4C5}",   // calendar
            (true, false) => "\u{1F4C5}",   // calendar only
            (false, true) => "\u{23F0}",    // clock only
            (false, false) => "\u{1F4C5}",  // default
        };

        // Build display text with appropriate icon.
        let display_text = format!("{} {}", icon, value);

        // Determine if this date-time input has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());

        // Build bordered block with label as title.
        let block_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let mut block = Block::default().borders(Borders::ALL).style(block_style);
        if !label.is_empty() {
            block = block.title(Span::styled(
                format!(" {} ", label),
                Style::default().fg(Color::White),
            ));
        }

        let content_area = block.inner(inner);
        frame.render_widget(block, inner);

        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        let paragraph = Paragraph::new(Line::from(Span::styled(
            display_text,
            Style::default().fg(Color::White),
        )));
        frame.render_widget(paragraph, content_area);
    }
}
