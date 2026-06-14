//! List component — renders children in a vertical or horizontal layout.

use ratatui::{Frame, layout::{Direction, Rect}};

use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::{Align, ChildList, Justify};
use crate::component_impl::TuiComponent;
use crate::components::row::{render_static_children, render_template_children};

/// List component implementation.
///
/// Lays out children vertically (default) or horizontally using weighted splitting.
/// Invisible container — no margin or padding.
pub struct ListComponent;

impl TuiComponent for ListComponent {
    fn name(&self) -> &'static str {
        "List"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        let children = match comp_model.children() {
            Some(c) => c,
            None => return,
        };

        // Determine direction: default is "vertical".
        let dir = list_direction(comp_model);

        let justify = comp_model
            .get_property::<Justify>("justify")
            .unwrap_or(Justify::Start);
        let align = comp_model
            .get_property::<Align>("align")
            .unwrap_or(Align::Stretch);

        match children {
            ChildList::Static(ids) => {
                render_static_children(
                    ctx, area, frame, render_child, measure_child,
                    &ids, justify, align, dir,
                );
            }
            ChildList::Template { component_id, path } => {
                render_template_children(
                    ctx, area, frame, render_child, measure_child,
                    &component_id, &path, justify, align, dir,
                );
            }
        }
    }

    /// Natural height: vertical → sum of children; horizontal → max of children.
    fn natural_height(
        &self,
        ctx: &ComponentContext,
        available_width: u16,
        measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id)?;
        let dir = list_direction(comp_model);
        let ids = match comp_model.children()? {
            ChildList::Static(ids) => ids,
            ChildList::Template { component_id, path } => {
                let count = match ctx.data_context.get(&path) {
                    Some(serde_json::Value::Array(arr)) => arr.len(),
                    _ => return None,
                };
                if count == 0 {
                    return Some(0);
                }
                let item_path = format!("{}/{}", path, 0);
                let one = measure_child(&component_id, &item_path, available_width)?;
                return match dir {
                    Direction::Vertical => Some(one.saturating_mul(count as u16)),
                    Direction::Horizontal => Some(one),
                };
            }
        };
        if ids.is_empty() {
            return Some(0);
        }
        // Static children inherit this component's base path (matters when this
        // component is itself a template instance rendered at a nested path).
        let base = ctx.data_context.base_path();
        match dir {
            Direction::Vertical => {
                let mut sum: u16 = 0;
                for id in &ids {
                    sum = sum.saturating_add(measure_child(id, base, available_width)?);
                }
                Some(sum)
            }
            Direction::Horizontal => {
                let mut max: u16 = 0;
                for id in &ids {
                    max = max.max(measure_child(id, base, available_width)?);
                }
                Some(max)
            }
        }
    }
}

/// Resolve a List's direction property (default vertical).
fn list_direction(
    comp_model: &a2ui_base::model::component_model::ComponentModel,
) -> Direction {
    let direction: Option<String> = comp_model.get_property("direction");
    match direction.as_deref() {
        Some("horizontal") => Direction::Horizontal,
        _ => Direction::Vertical,
    }
}
