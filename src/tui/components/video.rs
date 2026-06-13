//! Video component — renders a placeholder for videos in TUI.

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

/// Video component implementation.
///
/// TUI cannot play video. This component shows a placeholder
/// with the video URL: `[▶ video_url]`.
/// Applies a default 1-cell margin.
pub struct VideoComponent;

impl TuiComponent for VideoComponent {
    fn name(&self) -> &'static str {
        "Video"
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

        // Resolve posterUrl.
        let poster = comp_model.get_property::<DynamicString>("posterUrl")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds));

        let display_text = if !url.is_empty() { url } else { "video".to_string() };
        let placeholder = if let Some(ref poster_url) = poster {
            if !poster_url.is_empty() {
                format!("[\u{25B6} {} | poster: {}]", display_text, poster_url)
            } else {
                format!("[\u{25B6} {}]", display_text)
            }
        } else {
            format!("[\u{25B6} {}]", display_text)
        };

        let paragraph = Paragraph::new(Line::from(Span::styled(
            placeholder,
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, inner);
    }

    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // Video scales to fit its area; no intrinsic height — authors grow it
        // with `weight`.
        None
    }
}
