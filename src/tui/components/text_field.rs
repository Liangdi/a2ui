//! TextField component — renders a labeled text input display.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::core::event::{EventResult, InputEvent, InputKey};
use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicString;
use crate::tui::component_impl::TuiComponent;

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

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &crate::core::event::InputEvent,
    ) -> Option<crate::core::event::EventResult> {
        let comp_model = ctx.components.get(&ctx.component_id)?;

        // Get the value binding path.
        let value_ds = comp_model.get_property::<DynamicString>("value")?;
        let binding = match value_ds {
            DynamicString::Binding(b) => b,
            _ => return None,
        };

        let current = ctx.data_context.resolve_dynamic_string(
            &DynamicString::Binding(binding.clone()),
        );

        match event {
            InputEvent::KeyPress { key: InputKey::Char(c) } => {
                let new_value = format!("{}{}", current, c);
                Some(EventResult::DataUpdate {
                    path: binding.path.clone(),
                    value: serde_json::Value::String(new_value),
                })
            }
            InputEvent::KeyPress { key: InputKey::Backspace } => {
                let new_value = if let Some((idx, _)) = current.char_indices().next_back() {
                    &current[..idx]
                } else {
                    ""
                }
                .to_string();
                Some(EventResult::DataUpdate {
                    path: binding.path.clone(),
                    value: serde_json::Value::String(new_value),
                })
            }
            _ => None,
        }
    }
}

/// Replace each character with a bullet character for obscured display.
fn obscure_value(value: &str) -> String {
    value.chars().map(|_| '\u{2022}').collect()
}
