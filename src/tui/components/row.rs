//! Row component — horizontal layout container.

use ratatui::{Frame, layout::{Direction, Rect}};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::{Align, ChildList, Justify};
use crate::tui::component_impl::TuiComponent;
use crate::tui::layout_engine::{apply_align, apply_justify, weighted_split};

/// Row component implementation.
///
/// Lays out children horizontally using weighted splitting.
/// Invisible container — no margin or padding.
pub struct RowComponent;

impl TuiComponent for RowComponent {
    fn name(&self) -> &'static str {
        "Row"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
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
        let align = comp_model.get_property::<Align>("align").unwrap_or(Align::Start);

        match children {
            ChildList::Static(ids) => {
                render_static_children(
                    ctx, area, frame, render_child,
                    &ids, justify, align, Direction::Horizontal,
                );
            }
            ChildList::Template { component_id, path } => {
                render_template_children(
                    ctx, area, frame, render_child,
                    &component_id, &path, justify, align, Direction::Horizontal,
                );
            }
        }
    }
}

/// Render a static list of children with weighted layout (shared with Column).
pub(crate) fn render_static_children(
    ctx: &ComponentContext,
    area: Rect,
    frame: &mut Frame,
    render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
    ids: &[String],
    justify: Justify,
    align: Align,
    direction: Direction,
) {
    if ids.is_empty() {
        return;
    }

    // Collect weights: look up each child's weight property from the component model.
    let weights: Vec<Option<f64>> = ids
        .iter()
        .map(|id| {
            ctx.components
                .get(id)
                .and_then(|m| m.weight())
        })
        .collect();

    // Split the area.
    let rects = weighted_split(direction, area, &weights);

    // Build (rect, natural_size) pairs for justify.
    let items: Vec<(Rect, u16)> = rects
        .iter()
        .map(|r| {
            let size = match direction {
                Direction::Horizontal => r.width,
                Direction::Vertical => r.height,
            };
            (*r, size)
        })
        .collect();

    // Apply justify.
    let justified = apply_justify(justify, &items, area, direction);

    // Apply align and render each child.
    for (i, child_id) in ids.iter().enumerate() {
        let child_area = apply_align(align, justified[i], area, direction);
        render_child(child_id, child_area, frame, ""); // static children inherit parent's base_path
    }
}

/// Render template children by iterating over a data-bound array (shared with Column).
pub(crate) fn render_template_children(
    ctx: &ComponentContext,
    area: Rect,
    frame: &mut Frame,
    render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
    component_id: &str,
    path: &str,
    justify: Justify,
    align: Align,
    direction: Direction,
) {
    // Resolve the data array at the given path.
    let array = match ctx.data_context.get(path) {
        Some(serde_json::Value::Array(arr)) => arr,
        _ => return,
    };

    let count = array.len();
    if count == 0 {
        return;
    }

    // All template instances get equal weight (None = 1.0 default).
    let weights: Vec<Option<f64>> = vec![None; count];

    let rects = weighted_split(direction, area, &weights);

    let items: Vec<(Rect, u16)> = rects
        .iter()
        .map(|r| {
            let size = match direction {
                Direction::Horizontal => r.width,
                Direction::Vertical => r.height,
            };
            (*r, size)
        })
        .collect();

    let justified = apply_justify(justify, &items, area, direction);

    for i in 0..count {
        let child_area = apply_align(align, justified[i], area, direction);
        // Compute per-item nested path so each template instance resolves
        // data from its own array element.
        let item_path = format!("{}/{}", path, i);
        render_child(component_id, child_area, frame, &item_path);
    }
}
