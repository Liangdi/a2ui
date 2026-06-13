//! Text component — renders a styled paragraph.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicString;
use crate::tui::component_impl::TuiComponent;

/// Text component implementation.
///
/// Renders a `ratatui::widgets::Paragraph` with variant-based styling.
/// Applies a default margin of 1 cell on all sides (leaf component).
pub struct TextComponent;

impl TuiComponent for TextComponent {
    fn name(&self) -> &'static str {
        "Text"
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

        // Resolve the text content.
        let text_content = match comp_model.get_property::<DynamicString>("text") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Determine variant styling.
        let variant: Option<String> = comp_model.get_property("variant");
        let style = match variant.as_deref() {
            Some("h1") => Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(ratatui::style::Color::Cyan),
            Some("h2") => Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(ratatui::style::Color::Green),
            Some("h3") => Style::default().add_modifier(Modifier::BOLD),
            Some("h4") => Style::default().add_modifier(Modifier::UNDERLINED),
            Some("h5") => Style::default().add_modifier(Modifier::ITALIC),
            Some("caption") => Style::default().add_modifier(Modifier::DIM),
            Some("body") | None => Style::default(),
            _ => Style::default(),
        };

        // Apply default margin of 1 cell on all sides (leaf component per spec).
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let paragraph = Paragraph::new(text_content).style(style);
        frame.render_widget(paragraph, inner);
    }
}
