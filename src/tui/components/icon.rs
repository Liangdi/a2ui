//! Icon component — maps icon names to Unicode symbols and renders them.

use ratatui::{
    Frame,
    layout::Rect,
    widgets::Paragraph,
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicString;
use crate::tui::component_impl::TuiComponent;

/// Icon component implementation.
///
/// Maps common icon names (e.g. "mail", "settings") to Unicode symbols.
/// If no mapping is found, shows the first 2 characters of the name in brackets.
/// Rendered as a `Paragraph`.
/// Applies a default 1-cell margin.
pub struct IconComponent;

impl TuiComponent for IconComponent {
    fn name(&self) -> &'static str {
        "Icon"
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

        // Resolve the icon name via DynamicString (handles literals and {path: ...} bindings).
        let name = match comp_model.get_property::<DynamicString>("name") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => return,
        };
        let symbol = map_icon(&name);

        let paragraph = Paragraph::new(symbol);
        frame.render_widget(paragraph, inner);
    }
}

/// Map an icon name to a Unicode symbol.
///
/// If the name is not recognized, returns the first 2 characters of the name
/// enclosed in brackets.
fn map_icon(name: &str) -> String {
    let symbol = match name {
        "mail" => "✉",
        "send" => "➤",
        "search" => "🔍",
        "settings" => "⚙",
        "star" => "★",
        "accountCircle" => "👤",
        "home" => "🏠",
        "heart" => "♥",
        "check" => "✓",
        "close" => "✕",
        "add" => "+",
        "remove" => "−",
        "edit" => "✎",
        "delete" => "🗑",
        "refresh" => "⟳",
        "arrowBack" => "←",
        "arrowForward" => "→",
        "arrowUp" => "↑",
        "arrowDown" => "↓",
        "info" => "ℹ",
        "warning" => "⚠",
        "error" => "✗",
        "success" => "✔",
        _ => return fallback_icon(name),
    };
    symbol.to_string()
}

/// Generate a fallback icon from an unknown name: first 2 chars in brackets.
fn fallback_icon(name: &str) -> String {
    let chars: String = name.chars().take(2).collect();
    format!("[{}]", chars)
}
