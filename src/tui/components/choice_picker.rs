//! ChoicePicker component — renders a list of selectable options.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::{DynamicString, DynamicStringList};
use crate::tui::component_impl::TuiComponent;

/// An option entry in the choice picker.
#[derive(Debug, Clone, serde::Deserialize)]
struct ChoiceOption {
    label: String,
    #[serde(default)]
    value: String,
}

/// ChoicePicker component implementation.
///
/// Renders a list of options with radio buttons (mutuallyExclusive) or
/// checkboxes (multipleSelection). Supports "checkbox" and "chips" display
/// styles. Selected options are highlighted based on the resolved `value`.
/// Applies a default 1-cell margin.
pub struct ChoicePickerComponent;

impl TuiComponent for ChoicePickerComponent {
    fn name(&self) -> &'static str {
        "ChoicePicker"
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

        // Resolve options.
        let options: Vec<ChoiceOption> = match comp_model.get_property("options") {
            Some(opts) => opts,
            None => return,
        };

        // Resolve current value as a list of selected strings.
        let selected_values: Vec<String> = match comp_model.get_property::<DynamicStringList>("value")
        {
            Some(dsl) => match dsl {
                DynamicStringList::Literal(v) => v,
                DynamicStringList::Binding(b) => {
                    // Try to resolve as an array of strings from data model.
                    match ctx.data_context.get(&b.path) {
                        Some(serde_json::Value::Array(arr)) => arr
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect(),
                        _ => Vec::new(),
                    }
                }
                DynamicStringList::Function(fc) => {
                    // Execute function and try to get array of strings.
                    let result = ctx.data_context.resolve_dynamic_value(
                        &crate::core::protocol::common_types::DynamicValue::Function(fc),
                    );
                    match result {
                        serde_json::Value::Array(arr) => arr
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect(),
                        _ => Vec::new(),
                    }
                }
            },
            None => Vec::new(),
        };

        // Determine variant.
        let variant: Option<String> = comp_model.get_property("variant");
        let is_exclusive = variant.as_deref() == Some("mutuallyExclusive");

        // Build lines.
        let mut lines: Vec<Line> = Vec::new();

        // Add label line if present.
        if !label.is_empty() {
            lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(Color::White),
            )));
        }

        // Add option lines.
        for option in &options {
            let is_selected = selected_values.iter().any(|v| v == &option.value);

            let indicator = if is_exclusive {
                if is_selected {
                    "\u{25cf} " // ● filled circle
                } else {
                    "\u{25cb} " // ○ empty circle
                }
            } else {
                if is_selected {
                    "\u{2611} " // ☑ checked box
                } else {
                    "\u{2610} " // ☐ empty box
                }
            };

            let style = if is_selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            lines.push(Line::from(vec![
                Span::styled(indicator.to_string(), style),
                Span::styled(option.label.clone(), style),
            ]));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }
}
