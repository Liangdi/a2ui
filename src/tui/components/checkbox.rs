//! CheckBox component — renders a labeled checkbox with a checked/unchecked indicator.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Paragraph,
};

use crate::core::event::{EventResult, InputKey};
use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::{DynamicBoolean, DynamicString};
use crate::tui::component_impl::TuiComponent;

/// CheckBox component implementation.
///
/// Displays `[☑] Label text` or `[☐] Label text` based on the resolved
/// boolean value. Rendered as a `Paragraph`.
/// Applies a default 1-cell margin.
pub struct CheckBoxComponent;

impl TuiComponent for CheckBoxComponent {
    fn name(&self) -> &'static str {
        "CheckBox"
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

        // Resolve the label text.
        let label = match comp_model.get_property::<DynamicString>("label") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve the checked state.
        let checked = match comp_model.get_property::<DynamicBoolean>("value") {
            Some(db) => ctx.data_context.resolve_dynamic_boolean(&db),
            None => false,
        };

        // Format the display: [☑] Label or [☐] Label.
        let indicator = if checked { "☑" } else { "☐" };
        let display_text = format!("[{}] {}", indicator, label);

        // Determine if this checkbox has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());
        let style = if is_focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let paragraph = Paragraph::new(display_text).style(style);
        frame.render_widget(paragraph, inner);
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &crate::core::event::InputEvent,
    ) -> Option<crate::core::event::EventResult> {
        let crate::core::event::InputEvent::KeyPress { key } = event;
        if !matches!(key, InputKey::Enter | InputKey::Space) {
            return None;
        }

        let comp_model = ctx.components.get(&ctx.component_id)?;

        // Get the value binding to find the data path.
        let value = comp_model.get_property::<DynamicBoolean>("value")?;
        if let DynamicBoolean::Binding(binding) = value {
            return Some(EventResult::Toggle { path: binding.path });
        }
        None
    }
}
