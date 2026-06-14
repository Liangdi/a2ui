//! TextField component — renders a labeled text input display.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::DynamicString;
use crate::component_impl::TuiComponent;

/// TextField component implementation.
///
/// Renders a bordered paragraph with a label title and the current value
/// followed by a cursor block character. Supports variants:
/// - "shortText" (default): plain text display
/// - "longText": plain text display
/// - "number": plain text display
/// - "obscured": shows `*` for each character instead of actual text
///
/// Actual keyboard input handling is done at the Gallery App level;
/// this component only displays the current resolved value.
/// Applies a default 1-cell margin.
pub struct TextFieldComponent;

impl TuiComponent for TextFieldComponent {
    fn name(&self) -> &'static str {
        "TextField"
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

        // Resolve the label.
        let label = match comp_model.get_property::<DynamicString>("label") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve the current value.
        let raw_value = match comp_model.get_property::<DynamicString>("value") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Determine variant and mask the value if obscured.
        let variant: Option<String> = comp_model.get_property("variant");
        let display_value = match variant.as_deref() {
            Some("obscured") => obscure_value(&raw_value),
            _ => raw_value.clone(),
        };

        // Resolve placeholder.
        let placeholder = comp_model
            .get_property::<DynamicString>("placeholder")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds));

        // Build the display text: value followed by a cursor block character.
        // When value is empty and placeholder is set, show placeholder in dim color.
        let (display_text, is_placeholder) = if raw_value.is_empty() {
            match &placeholder {
                Some(p) if !p.is_empty() => (p.clone(), true),
                _ => ("\u{2588}".to_string(), false), // cursor block only
            }
        } else {
            (format!("{}\u{2588}", display_value), false)
        };

        // Determine if this text field has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());

        // Build the bordered block with the label as title.
        // When focused, use yellow border to indicate focus.
        let block_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(label)
            .style(block_style);

        let content_area = block.inner(inner);

        // Render the block.
        frame.render_widget(block, inner);

        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        // Render the paragraph with the display text.
        let paragraph_style = if is_placeholder {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };
        let paragraph = Paragraph::new(Line::from(Span::styled(display_text, paragraph_style)));
        frame.render_widget(paragraph, content_area);
    }

    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // 1 input line + 2-cell margin + 2-cell border = 5. The render does
        // `inner = area.shrink(1)` (margin) then `Block::bordered()` (border), so a
        // single content line needs area.height - 4 >= 1 → minimum 5 rows.
        Some(5)
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &a2ui_base::event::InputEvent,
    ) -> Option<a2ui_base::event::EventResult> {
        a2ui_base::components::text_field::handle_event(ctx, event)
    }
}

/// Replace each character with a bullet character for obscured display.
fn obscure_value(value: &str) -> String {
    value.chars().map(|_| '\u{2022}').collect()
}
