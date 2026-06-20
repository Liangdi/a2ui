//! CheckBox component — renders a labeled checkbox with a checked/unchecked indicator.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Paragraph,
};

use crate::component_impl::TuiComponent;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::{DynamicBoolean, DynamicString};

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
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let paragraph = Paragraph::new(display_text).style(style);
        frame.render_widget(paragraph, inner);
    }

    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // Single-line `[☑] label` content + 2-cell margin.
        Some(3)
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &a2ui_base::event::InputEvent,
    ) -> Option<a2ui_base::event::EventResult> {
        a2ui_base::components::checkbox::handle_event(ctx, event)
    }
}
