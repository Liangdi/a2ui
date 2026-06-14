//! Row component — horizontal layout container.

use ratatui::{Frame, layout::{Direction, Rect}};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::{Align, ChildList, Justify};
use crate::tui::component_impl::TuiComponent;
use crate::tui::layout_engine::{apply_align, flex_layout};

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
        let align = comp_model.get_property::<Align>("align").unwrap_or(Align::Start);

        match children {
            ChildList::Static(ids) => {
                render_static_children(
                    ctx, area, frame, render_child, measure_child,
                    &ids, justify, align, Direction::Horizontal,
                );
            }
            ChildList::Template { component_id, path } => {
                render_template_children(
                    ctx, area, frame, render_child, measure_child,
                    &component_id, &path, justify, align, Direction::Horizontal,
                );
            }
        }
    }

    /// A Row's height = the tallest child's natural height (cross axis).
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
                let item_path = format!("{}/{}", path, 0);
                return measure_child(&component_id, &item_path, available_width);
            }
        };
        if ids.is_empty() {
            return Some(0);
        }
        // Static children inherit this component's base path (matters when this
        // component is itself a template instance rendered at a nested path).
        let base = ctx.data_context.base_path();
        let mut max: u16 = 0;
        for id in &ids {
            max = max.max(measure_child(id, base, available_width)?);
        }
        Some(max)
    }
}

/// Render a static list of children with flexbox layout (shared with Column/Row/List).
///
/// On the **vertical** main axis, each child is measured for its natural height and the
/// axis is distributed by natural size + flex-grow (`weight`); leftover space is placed
/// per `justify`. On the **horizontal** main axis, natural width is not measured, so
/// children are distributed by weight (legacy behavior); their cross-axis (height) is
/// then handled by `align`.
pub(crate) fn render_static_children(
    ctx: &ComponentContext,
    area: Rect,
    frame: &mut Frame,
    render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
    measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ids: &[String],
    justify: Justify,
    align: Align,
    direction: Direction,
) {
    if ids.is_empty() {
        return;
    }

    // Static children inherit the parent's current base path so a static list
    // nested inside a template instance still resolves bindings against the item's
    // data scope (e.g. a template card rendered at /items/0 → [title, subtitle]).
    let base = ctx.data_context.base_path().to_string();

    // Build (natural_main_size, weight) per child. Only the vertical main axis has a
    // measured natural size (height); horizontal relies on weight distribution.
    let items: Vec<(Option<u16>, Option<f64>)> = ids
        .iter()
        .map(|id| {
            let weight = ctx.components.get(id).and_then(|m| m.weight());
            let natural = match direction {
                Direction::Vertical => measure_child(id, &base, area.width),
                Direction::Horizontal => None,
            };
            (natural, weight)
        })
        .collect();

    let rects = flex_layout(direction, area, &items, justify);

    // Apply cross-axis alignment and render each child.
    for (i, child_id) in ids.iter().enumerate() {
        let child_area = apply_align(align, rects[i], area, direction);
        render_child(child_id, child_area, frame, &base);
    }
}

/// Render template children by iterating over a data-bound array (shared with Column/Row/List).
pub(crate) fn render_template_children(
    ctx: &ComponentContext,
    area: Rect,
    frame: &mut Frame,
    render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
    measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
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

    // Measure each instance at its own item path so data-dependent heights (e.g. option
    // counts) resolve correctly. Horizontal main axis has no measured natural width.
    let items: Vec<(Option<u16>, Option<f64>)> = (0..count)
        .map(|i| {
            let item_path = format!("{}/{}", path, i);
            let natural = match direction {
                Direction::Vertical => measure_child(component_id, &item_path, area.width),
                Direction::Horizontal => None,
            };
            // Template instances carry no explicit weight → equal share / legacy fill.
            (natural, None)
        })
        .collect();

    let rects = flex_layout(direction, area, &items, justify);

    for i in 0..count {
        let child_area = apply_align(align, rects[i], area, direction);
        // Per-item nested path so each template instance resolves its own array element.
        let item_path = format!("{}/{}", path, i);
        render_child(component_id, child_area, frame, &item_path);
    }
}
