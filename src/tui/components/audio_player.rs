//! AudioPlayer component — renders a placeholder for audio in TUI.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicString;
use crate::tui::component_impl::TuiComponent;

/// AudioPlayer component implementation.
///
/// TUI cannot play audio. This component shows a placeholder
/// with the audio URL: `[♫ audio_url]`.
/// Applies a default 1-cell margin.
pub struct AudioPlayerComponent;

impl TuiComponent for AudioPlayerComponent {
    fn name(&self) -> &'static str {
        "AudioPlayer"
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

        // Resolve URL.
        let url = match comp_model.get_property::<DynamicString>("url") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        let display_text = if !url.is_empty() { url } else { "audio".to_string() };
        let placeholder = format!("[\u{266B} {}]", display_text);

        let paragraph = Paragraph::new(Line::from(Span::styled(
            placeholder,
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, inner);
    }
}
