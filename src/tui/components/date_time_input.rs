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
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame),
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

        // Build display text with calendar icon.
        let display_text = format!("\u{1F4C5} {}", value);

        // Build bordered block with label as title.
        let mut block = Block::default().borders(Borders::ALL);
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
