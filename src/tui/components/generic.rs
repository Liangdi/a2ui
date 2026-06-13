//! Generic fallback component.
//!
//! When the renderer encounters a component type that has no registered native
//! [`TuiComponent`](crate::tui::component_impl::TuiComponent) (for example, a
//! component declared in an *inline catalog* that the client received but did
//! not implement natively), it falls back to [`GenericComponent`].
//!
//! The generic renderer draws a bordered block titled with the (unknown)
//! component type, lists every property as `key: resolved-value`, and then
//! renders any `child`/`children` below the property dump so nested trees are
//! still visible. It never panics on missing fields.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::{ChildList, DynamicString};
use crate::tui::component_impl::TuiComponent;

/// A stateless, zero-sized fallback renderer for unknown component types.
pub struct GenericComponent;

impl TuiComponent for GenericComponent {
    fn name(&self) -> &'static str {
        "Generic"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        // --- Build the block + property dump ---
        let title = format!("{} (unknown)", comp_model.component_type);
        let block = Block::bordered()
            .title(title)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().fg(Color::Gray));

        let inner = block.inner(area);
        // Render the block first so the border is always drawn.
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Build lines of "key: value" for each property, resolving
        // DynamicString values that look like a string binding/function.
        let mut lines: Vec<Line> = Vec::new();
        for (key, val) in comp_model.properties.iter() {
            let resolved = resolve_value_for_display(val, ctx);
            let line = Line::from(vec![
                Span::styled(format!("{key}: "), Style::default().fg(Color::Cyan)),
                Span::raw(resolved),
            ]);
            lines.push(line);
        }

        if lines.is_empty() {
            lines.push(Line::from("(no properties)").alignment(Alignment::Center));
        }

        // Reserve the bottom row(s) for children if present.
        let child_ids = collect_child_ids(comp_model);
        let child_row_count = if child_ids.is_empty() { 0 } else { 1 };

        let prop_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(child_row_count),
        };

        if prop_area.height > 0 {
            frame.render_widget(Paragraph::new(lines), prop_area);
        }

        // --- Render children (if any) in a single stacked row below ---
        if !child_ids.is_empty() && child_row_count > 0 && inner.height > child_row_count {
            let child_area = Rect {
                x: inner.x,
                y: inner.y + prop_area.height,
                width: inner.width,
                height: child_row_count as u16,
            };
            // Give each child an equal horizontal slice.
            let count = child_ids.len() as u16;
            let slice_w = child_area.width / count.max(1);
            for (i, cid) in child_ids.iter().enumerate() {
                let ca = Rect {
                    x: child_area.x + (slice_w * i as u16),
                    y: child_area.y,
                    width: slice_w,
                    height: child_area.height,
                };
                if ca.width > 0 && ca.height > 0 {
                    render_child(cid, ca, frame, "");
                }
            }
        }
    }

    fn natural_height(
        &self,
        ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id)?;
        let prop_count = comp_model.properties.len().max(1) as u16;
        let has_children = comp_model.child().is_some()
            || matches!(comp_model.children(), Some(crate::core::protocol::common_types::ChildList::Static(v)) if !v.is_empty());
        let mut h = prop_count.saturating_add(2);
        if has_children {
            h = h.saturating_add(1);
        }
        Some(h)
    }
}

/// Render a property value as a human-readable string. String-typed dynamic
/// values are resolved through the data context; everything else is shown as
/// its raw JSON so the developer can see exactly what the server sent.
fn resolve_value_for_display(
    val: &serde_json::Value,
    ctx: &ComponentContext,
) -> String {
    // If it's a string, it might be a DynamicString (literal/binding/function).
    if let serde_json::Value::String(_) = val {
        if let Ok(ds) = serde_json::from_value::<DynamicString>(val.clone()) {
            return ctx.data_context.resolve_dynamic_string(&ds);
        }
    }
    // Fall back to the raw JSON representation.
    val.to_string()
}

/// Collect the IDs of any `child` (single) or `children` (list) to render
/// beneath the property dump. Robust to missing/malformed fields.
fn collect_child_ids(
    comp_model: &crate::core::model::component_model::ComponentModel,
) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(single) = comp_model.child() {
        ids.push(single);
    }
    if let Some(list) = comp_model.children() {
        match list {
            ChildList::Static(v) => ids.extend(v),
            // Templates can't be expanded here without data iteration; skip.
            ChildList::Template { .. } => {}
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::component_model::ComponentModel;
    use serde_json::json;

    #[test]
    fn collect_child_ids_single() {
        let cm = ComponentModel::from_json(&json!({
            "id": "x",
            "component": "Mystery",
            "child": "label"
        }))
        .unwrap();
        assert_eq!(collect_child_ids(&cm), vec!["label".to_string()]);
    }

    #[test]
    fn collect_child_ids_list() {
        let cm = ComponentModel::from_json(&json!({
            "id": "x",
            "component": "Mystery",
            "children": ["a", "b"]
        }))
        .unwrap();
        assert_eq!(collect_child_ids(&cm), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn collect_child_ids_empty() {
        let cm = ComponentModel::from_json(&json!({
            "id": "x",
            "component": "Mystery"
        }))
        .unwrap();
        assert!(collect_child_ids(&cm).is_empty());
    }

    #[test]
    fn name_is_generic() {
        assert_eq!(GenericComponent.name(), "Generic");
    }
}
