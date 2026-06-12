//! TextField component — renders a labeled text input display.

use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

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
            _ => raw_value,
        };

        // Build the display text: value followed by a cursor block character.
        let display_text = format!("{}\u{2588}", display_value);

        // Build the bordered block with the label as title.
        let block = Block::default()
            .borders(Borders::ALL)
            .title(label);

        let content_area = block.inner(inner);

        // Render the block.
        frame.render_widget(block, inner);

        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        // Render the paragraph with the display text.
        let paragraph = Paragraph::new(Line::from(display_text));
        frame.render_widget(paragraph, content_area);
    }
}

/// Replace each character with a bullet character for obscured display.
fn obscure_value(value: &str) -> String {
    value.chars().map(|_| '\u{2022}').collect()
}
