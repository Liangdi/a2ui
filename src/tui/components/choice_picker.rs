//! ChoicePicker component — renders a list of selectable options.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::core::event::{EventResult, InputEvent, InputKey};
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
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        // Apply default 1-cell margin on all sides (never collapses to zero).
        let inner = crate::tui::layout_engine::padded_content(area);

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

        // Determine display style and filterable flag.
        let display_style: Option<String> = comp_model.get_property("displayStyle");
        let _filterable: bool = comp_model.get_property("filterable").unwrap_or(false);
        let is_chips = display_style.as_deref() == Some("chips");

        // Determine if this choice picker has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());

        // Build lines.
        let mut lines: Vec<Line> = Vec::new();

        // Add label line if present.
        if !label.is_empty() {
            let label_style = if is_focused {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(label, label_style)));
        }

        // Add option lines.
        if is_chips {
            // Render as inline chips: [✓ Email] [○ Phone] [○ SMS]
            let mut spans = Vec::new();
            for (i, option) in options.iter().enumerate() {
                let is_selected = selected_values.iter().any(|v| v == &option.value);
                let indicator = if is_exclusive {
                    if is_selected { "◉ " } else { "○ " }
                } else {
                    if is_selected { "☑ " } else { "☐ " }
                };
                let style = if is_selected {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(format!("{}{}", indicator, option.label), style));
            }
            lines.push(Line::from(spans));
        } else {
            // Default stacked layout
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
        }

        let paragraph = Paragraph::new(lines);

        // When focused, render with a yellow bordered block.
        if is_focused {
            let block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow));
            let content_area = block.inner(inner);
            frame.render_widget(block, inner);
            frame.render_widget(paragraph, content_area);
        } else {
            frame.render_widget(paragraph, inner);
        }
    }

    fn natural_height(
        &self,
        ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id)?;

        // Resolve label.
        let label = match comp_model.get_property::<DynamicString>("label") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve options.
        let options: Vec<ChoiceOption> = comp_model.get_property("options")?;

        // Determine display style.
        let display_style: Option<String> = comp_model.get_property("displayStyle");
        let is_chips = display_style.as_deref() == Some("chips");

        let lines = (if !label.is_empty() { 1 } else { 0 })
            + (if is_chips { 1 } else { options.len() });

        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());

        Some((lines as u16).saturating_add(2).saturating_add(if is_focused { 2 } else { 0 }))
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &crate::core::event::InputEvent,
    ) -> Option<crate::core::event::EventResult> {
        let comp_model = ctx.components.get(&ctx.component_id)?;

        let options: Vec<ChoiceOption> = comp_model.get_property("options")?;
        if options.is_empty() {
            return None;
        }

        let variant: Option<String> = comp_model.get_property("variant");
        let is_exclusive = variant.as_deref() == Some("mutuallyExclusive");

        let value_dsl = comp_model.get_property::<DynamicStringList>("value")?;
        let (binding, selected) = match &value_dsl {
            DynamicStringList::Binding(b) => {
                let selected = match ctx.data_context.get(&b.path) {
                    Some(serde_json::Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>(),
                    Some(serde_json::Value::String(s)) => vec![s.clone()],
                    _ => Vec::new(),
                };
                (b.clone(), selected)
            }
            _ => return None,
        };

        match event {
            InputEvent::KeyPress { key: InputKey::Down } | InputEvent::KeyPress { key: InputKey::Up } => {
                if !is_exclusive {
                    return None;
                }
                // Find current selection index, move to next/prev.
                let current_idx = selected
                    .first()
                    .and_then(|v| options.iter().position(|o| &o.value == v))
                    .unwrap_or(0);
                let new_idx = match event {
                    InputEvent::KeyPress { key: InputKey::Down } => {
                        (current_idx + 1) % options.len()
                    }
                    InputEvent::KeyPress { key: InputKey::Up } => {
                        if current_idx == 0 {
                            options.len() - 1
                        } else {
                            current_idx - 1
                        }
                    }
                    _ => current_idx,
                };
                Some(EventResult::DataUpdate {
                    path: binding.path.clone(),
                    value: serde_json::json!([options[new_idx].value]),
                })
            }
            InputEvent::KeyPress { key: InputKey::Enter } | InputEvent::KeyPress { key: InputKey::Space } => {
                if is_exclusive {
                    return None;
                } // handled by Up/Down for exclusive
                  // For multiple selection: not enough state to know which option to toggle.
                  // Skip for now.
                None
            }
            _ => None,
        }
    }
}
