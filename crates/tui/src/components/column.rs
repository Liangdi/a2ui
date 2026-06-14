//! Column component — vertical layout container.

use ratatui::{Frame, layout::{Direction, Rect}};

use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::{Align, ChildList, Justify};
use crate::component_impl::TuiComponent;
use crate::components::row::{render_static_children, render_template_children};

/// Column component implementation.
///
/// Lays out children vertically using weighted splitting.
/// Invisible container — no margin or padding.
/// Default justify: Start, default align: Stretch.
pub struct ColumnComponent;

impl TuiComponent for ColumnComponent {
    fn name(&self) -> &'static str {
        "Column"
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

        let justify = comp_model.get_property::<Justify>("justify").unwrap_or(Justify::Start);
        let align = comp_model.get_property::<Align>("align").unwrap_or(Align::Stretch);

        match children {
            ChildList::Static(ids) => {
                render_static_children(
                    ctx, area, frame, render_child, measure_child,
                    &ids, justify, align, Direction::Vertical,
                );
            }
            ChildList::Template { component_id, path } => {
                render_template_children(
                    ctx, area, frame, render_child, measure_child,
                    &component_id, &path, justify, align, Direction::Vertical,
                );
            }
        }
    }

    /// Natural height = sum of children's natural heights (vertical stack).
    /// `None` if there are no children or any child reports `None` (conservative —
    /// a container with an unmeasured child cannot give a definite height).
    fn natural_height(
        &self,
        ctx: &ComponentContext,
        available_width: u16,
        measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id)?;
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
                // All instances share one component_id; measure the first at its path
                // and multiply (heights are structurally identical).
                let item_path = format!("{}/{}", path, 0);
                let one = measure_child(&component_id, &item_path, available_width)?;
                return Some(one.saturating_mul(count as u16));
            }
        };
        if ids.is_empty() {
            return Some(0);
        }
        // Static children inherit this component's base path (matters when this
        // component is itself a template instance rendered at a nested path).
        let base = ctx.data_context.base_path();
        let mut sum: u16 = 0;
        for id in &ids {
            sum = sum.saturating_add(measure_child(id, base, available_width)?);
        }
        Some(sum)
    }
}
