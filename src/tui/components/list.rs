//! List component — renders children in a vertical or horizontal layout.

use ratatui::{Frame, layout::{Direction, Rect}};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::{Align, ChildList, Justify};
use crate::tui::component_impl::TuiComponent;
use crate::tui::components::row::{render_static_children, render_template_children};

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
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame),
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
        let direction: Option<String> = comp_model.get_property("direction");
        let dir = match direction.as_deref() {
            Some("horizontal") => Direction::Horizontal,
            _ => Direction::Vertical,
        };

        let justify = comp_model
            .get_property::<Justify>("justify")
            .unwrap_or(Justify::Start);
        let align = comp_model
            .get_property::<Align>("align")
            .unwrap_or(Align::Stretch);

        match children {
            ChildList::Static(ids) => {
                render_static_children(
                    ctx, area, frame, render_child,
                    &ids, justify, align, dir,
                );
            }
            ChildList::Template { component_id, path } => {
                render_template_children(
                    ctx, area, frame, render_child,
                    &component_id, &path, justify, align, dir,
                );
            }
        }
    }
}
