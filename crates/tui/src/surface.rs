//! Surface renderer — entry point for rendering an A2UI component tree into a ratatui frame.

use std::collections::HashMap;

use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    widgets::{Block, Clear, Paragraph},
};

use super::component_impl::ComponentRegistry;
use super::component_impl::TuiComponent;
use a2ui_base::catalog::Catalog;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::model::surface_model::SurfaceModel;
use a2ui_base::protocol::common_types::DynamicBoolean;

/// Renders a [`SurfaceModel`] into a ratatui frame by walking the component tree.
pub struct SurfaceRenderer<'a> {
    surface: &'a SurfaceModel,
    registry: &'a ComponentRegistry,
    catalog: &'a Catalog,
}

impl<'a> SurfaceRenderer<'a> {
    /// Create a new renderer for the given surface.
    pub fn new(
        surface: &'a SurfaceModel,
        registry: &'a ComponentRegistry,
        catalog: &'a Catalog,
    ) -> Self {
        Self {
            surface,
            registry,
            catalog,
        }
    }

    /// Main entry point: render the component tree into the frame.
    ///
    /// Renders the tree (see `render_tree`) and then paints any open
    /// [`Modal`](a2ui_base::model::component_model::ComponentModel) as a centered
    /// overlay on top.
    pub fn render(&self, frame: &mut Frame, area: Rect, focused_id: Option<&str>) {
        self.render_tree(frame, area, focused_id);

        // Any open Modal floats its content as a centered overlay on top of the
        // rendered tree — a real modal dialog rather than an inline swap.
        let data_model = self.surface.data_model.borrow();
        let components = self.surface.components.borrow();
        let surface_id = &self.surface.id;
        self.render_modal_overlays(
            frame,
            area,
            surface_id,
            &data_model,
            &components,
            focused_id,
        );
    }

    /// Render the tree with viewport scrolling, **without squishing the content**.
    ///
    /// A layout-container root (`Column`/`Row`/`List`) *fills* whatever rect it
    /// is given (see [`Self::render`]). That is desirable for a full-screen app,
    /// but wrong inside a scroll viewport: if you hand the surface only its
    /// visible slice, the layout engine reflows/shrinks the whole tree into that
    /// slice and the content deforms as soon as it is partially scrolled.
    ///
    /// `render_scrolled` instead lays the surface out **once, at its full natural
    /// height**, into an off-screen buffer, then paints the slice
    /// `[scroll_offset, scroll_offset + viewport.height)` into `viewport`. Content
    /// above `scroll_offset` (scrolled out the top) is clipped. Because layout
    /// never depends on the viewport size, scrolling always reveals a true,
    /// un-squished slice.
    ///
    /// This intentionally does **not** paint [`Modal`] overlays — a modal is a
    /// full-surface floating dialog, which is meaningless inside a clipped slice.
    /// Use [`Self::render`] when you want modals.
    ///
    /// If the root's natural height cannot be measured, falls back to [`Self::render`].
    ///
    /// [`Modal`]: a2ui_base::model::component_model::ComponentModel
    pub fn render_scrolled(
        &self,
        frame: &mut Frame,
        viewport: Rect,
        scroll_offset: usize,
        focused_id: Option<&str>,
    ) {
        if viewport.width == 0 || viewport.height == 0 {
            return;
        }
        // Fall back to the plain fill-render when we cannot size the content.
        let natural = match self.measure(viewport.width) {
            Some(n) => n,
            None => {
                self.render(frame, viewport, focused_id);
                return;
            }
        };
        if natural == 0 {
            return;
        }
        // Nothing left to show (scrolled past the end).
        let scroll_offset = scroll_offset.min(natural as usize);
        if scroll_offset as u16 >= natural {
            return;
        }

        // Off-screen render at full natural height. A container root fills this
        // rect exactly, so children keep their natural sizes — no shrink/reflow.
        // TestBackend never errors, so this construction is infallible.
        let scratch_backend = TestBackend::new(viewport.width, natural);
        let mut scratch = Terminal::new(scratch_backend).unwrap();
        let _ = scratch.draw(|f| self.render_tree(f, f.area(), focused_id));
        let src = scratch.backend().buffer();

        // Blit only the visible rows into the live frame, clamped to the viewport.
        let dst = frame.buffer_mut();
        let vis_h = ((natural as usize) - scroll_offset).min(viewport.height as usize);
        let bottom = viewport.bottom();
        for row in 0..vis_h {
            let sy = (scroll_offset + row) as u16;
            let dy = viewport.y + row as u16;
            if dy >= bottom {
                break;
            }
            for col in 0..viewport.width {
                let dx = viewport.x + col;
                if let (Some(src_cell), Some(dst_cell)) =
                    (src.cell((col, sy)), dst.cell_mut((dx, dy)))
                {
                    *dst_cell = src_cell.clone();
                }
            }
        }
    }

    /// Render the component tree (root sizing + recursive [`render_node`]), **without**
    /// modal overlays. Shared by [`Self::render`] (which paints modals on top) and
    /// [`render_scrolled`] (which renders into an off-screen buffer where a modal
    /// overlay would be meaningless).
    fn render_tree(&self, frame: &mut Frame, area: Rect, focused_id: Option<&str>) {
        let data_model = self.surface.data_model.borrow();
        let components = self.surface.components.borrow();
        let surface_id = &self.surface.id;

        // Look up the root component.
        if !components.contains("root") {
            let widget = Paragraph::new("No root component").block(Block::bordered());
            frame.render_widget(widget, area);
            return;
        }

        // Root sizing: layout containers (Column/Row/List) fill the viewport so a
        // full-screen app just uses one of them as root; every other (content)
        // component — Card, Text, TextField, … — shrink-wraps to its natural height
        // and centers vertically. Full width is kept (natural width is not measured).
        let root_is_container = matches!(
            components.get("root").map(|m| m.component_type.as_str()),
            Some("Column") | Some("Row") | Some("List")
        );
        let root_area = if root_is_container {
            area
        } else {
            match measure_node(
                "root",
                surface_id,
                "",
                area.width,
                &data_model,
                &components,
                self.registry,
                &self.catalog.functions,
                focused_id,
            ) {
                Some(natural) => {
                    let h = natural.min(area.height);
                    // Top-anchor when content overflows (natural > panel height) so the
                    // top of the content stays visible; otherwise center vertically.
                    let y = if natural > area.height {
                        area.y
                    } else {
                        area.y + area.height.saturating_sub(h) / 2
                    };
                    Rect {
                        x: area.x,
                        y,
                        width: area.width,
                        height: h,
                    }
                }
                None => area,
            }
        };

        render_node(
            "root",
            surface_id,
            "",
            root_area,
            frame,
            &data_model,
            &components,
            self.registry,
            &self.catalog.functions,
            focused_id,
        );
    }

    /// Measure the root component's natural content height (including its own
    /// chrome: margins/borders), given `available_width` cells.
    ///
    /// Layout-container roots (Column/Row/List) return the sum/max of their
    /// children's natural heights; unknown/unmeasurable roots return `None`.
    /// This is the public counterpart of the internal measure pass, so callers
    /// that stack surfaces (e.g. a chat UI) can size each surface to its content.
    pub fn measure(&self, available_width: u16) -> Option<u16> {
        let data_model = self.surface.data_model.borrow();
        let components = self.surface.components.borrow();
        let surface_id = &self.surface.id;
        if !components.contains("root") {
            return None;
        }
        measure_node(
            "root",
            surface_id,
            "",
            available_width,
            &data_model,
            &components,
            self.registry,
            &self.catalog.functions,
            None,
        )
    }

    /// Convenience method to render a child by ID with an explicit base path.
    ///
    /// Useful for template-based rendering where a container iterates over a
    /// data array and renders the same component for each item with a nested
    /// data path.
    #[allow(clippy::too_many_arguments)] // recursive node renderer: ids + area + frame + shared model views
    pub fn render_child_by_id(
        &self,
        child_id: &str,
        surface_id: &str,
        base_path: &str,
        area: Rect,
        frame: &mut Frame,
        data_model: &DataModel,
        components: &SurfaceComponentsModel,
        focused_id: Option<&str>,
    ) {
        render_node(
            child_id,
            surface_id,
            base_path,
            area,
            frame,
            data_model,
            components,
            self.registry,
            &self.catalog.functions,
            focused_id,
        );
    }

    /// Draw each open Modal's `content` as a centered, bordered overlay on top of
    /// the already-rendered surface (with a dimmed backdrop), so a modal reads as
    /// a floating dialog rather than replacing its trigger inline.
    fn render_modal_overlays(
        &self,
        frame: &mut Frame,
        area: Rect,
        surface_id: &str,
        data_model: &DataModel,
        components: &SurfaceComponentsModel,
        focused_id: Option<&str>,
    ) {
        for m in components.all().values() {
            if m.component_type != "Modal" || !is_modal_open(m) {
                continue;
            }
            let Some(content_id) = m.get_property::<String>("content") else {
                continue;
            };

            // Dimmed backdrop over the whole surface: Clear erases the
            // underlying symbols (e.g. the trigger) first, then Block paints a
            // dark background (Block sets bg but doesn't erase symbols alone).
            frame.render_widget(Clear, area);
            frame.render_widget(
                Block::default().style(Style::default().bg(Color::Black)),
                area,
            );

            // Centered dialog box.
            let dialog = centered_rect(area, 60, 40);
            frame.render_widget(Clear, dialog);
            frame.render_widget(
                Block::bordered()
                    .title(" Modal ")
                    .style(Style::default().bg(Color::Gray)),
                dialog,
            );
            let inner = dialog.inner(Margin {
                horizontal: 1,
                vertical: 1,
            });
            if inner.width > 0 && inner.height > 0 {
                self.render_child_by_id(
                    &content_id,
                    surface_id,
                    "",
                    inner,
                    frame,
                    data_model,
                    components,
                    focused_id,
                );
            }
        }
    }
}

/// Whether a Modal's `isOpen` is a literal `true` — the form the gallery writes
/// locally when a trigger is activated. Bindings would need a data context to
/// resolve and aren't driven by the gallery's local interaction.
fn is_modal_open(m: &a2ui_base::model::component_model::ComponentModel) -> bool {
    matches!(
        m.get_property::<DynamicBoolean>("isOpen"),
        Some(DynamicBoolean::Literal(true))
    )
}

/// A rect centered in `area`, `width_pct`% wide and `height_pct`% tall.
fn centered_rect(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let pop = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(pop[1])[1]
}

/// Recursively render a single component node.
///
/// This free function is the core of the renderer. Each call:
/// 1. Looks up the component model by ID.
/// 2. Builds a [`ComponentContext`] for it.
/// 3. Finds the matching [`TuiComponent`](super::component_impl::TuiComponent) in the registry.
/// 4. Passes a `render_child` closure that re-enters this same function for any children.
#[allow(clippy::too_many_arguments)] // recursive walk: ids + area + frame + shared model views + focus
fn render_node(
    component_id: &str,
    surface_id: &str,
    base_path: &str,
    area: Rect,
    frame: &mut Frame,
    data_model: &DataModel,
    components: &SurfaceComponentsModel,
    registry: &ComponentRegistry,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
) {
    let comp_model = match components.get(component_id) {
        Some(m) => m,
        None => {
            let msg = format!("Component not found: {}", component_id);
            let widget = Paragraph::new(msg).block(Block::bordered());
            frame.render_widget(widget, area);
            return;
        }
    };

    let ctx = ComponentContext::new(
        component_id.to_string(),
        surface_id.to_string(),
        data_model,
        components,
        functions,
        base_path,
        focused_id.map(|s| s.to_string()),
    );

    // The render_child closure simply re-enters render_node for each child,
    // giving unbounded recursion depth without code duplication.
    //
    // Defined before the registry lookup so the generic fallback renderer can
    // also recurse into any child/children of an unknown component type.
    let mut render_child =
        |child_id: &str, child_area: Rect, child_frame: &mut Frame, child_base_path: &str| {
            render_node(
                child_id,
                surface_id,
                child_base_path,
                child_area,
                child_frame,
                data_model,
                components,
                registry,
                functions,
                focused_id,
            );
        };

    // The measure_child closure re-enters measure_node so containers can query a
    // child's natural height while laying out (render) and while measuring self.
    let mut measure_child =
        |child_id: &str, child_base_path: &str, available_width: u16| -> Option<u16> {
            measure_node(
                child_id,
                surface_id,
                child_base_path,
                available_width,
                data_model,
                components,
                registry,
                functions,
                focused_id,
            )
        };

    let tui_comp = match registry.get(&comp_model.component_type) {
        Some(c) => c,
        None => {
            // No native renderer for this component type (e.g. a component
            // declared in an inline catalog). Fall back to the generic renderer
            // so the tree is still visible instead of a static "unknown" stub.
            super::components::GenericComponent.render(
                &ctx,
                area,
                frame,
                &mut render_child,
                &mut measure_child,
            );
            return;
        }
    };

    tui_comp.render(&ctx, area, frame, &mut render_child, &mut measure_child);
}

/// Measure a single component node's natural height (measure pass counterpart of
/// [`render_node`]). Builds a child context, dispatches to the registered
/// [`TuiComponent`](super::component_impl::TuiComponent)'s `natural_height`, and
/// applies the component's optional `minHeight` floor centrally. Returns `None`
/// for unknown component types (treated as legacy fill by callers).
#[allow(clippy::too_many_arguments)] // recursive measure: mirrors render_node's signature
fn measure_node(
    component_id: &str,
    surface_id: &str,
    base_path: &str,
    available_width: u16,
    data_model: &DataModel,
    components: &SurfaceComponentsModel,
    registry: &ComponentRegistry,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
    focused_id: Option<&str>,
) -> Option<u16> {
    let comp_model = components.get(component_id)?;
    let ctx = ComponentContext::new(
        component_id.to_string(),
        surface_id.to_string(),
        data_model,
        components,
        functions,
        base_path,
        focused_id.map(|s| s.to_string()),
    );

    let tui_comp = registry.get(&comp_model.component_type)?;

    let mut measure_child = |child_id: &str, child_base_path: &str, width: u16| -> Option<u16> {
        measure_node(
            child_id,
            surface_id,
            child_base_path,
            width,
            data_model,
            components,
            registry,
            functions,
            focused_id,
        )
    };

    let mut height = tui_comp.natural_height(&ctx, available_width, &mut measure_child);

    // Central minHeight floor (total footprint, incl. margins/borders).
    if let Some(min) = comp_model.min_height() {
        height = Some(height.unwrap_or(0).max(min));
    }
    height
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::catalogs::basic::{build_basic_catalog, build_basic_registry};
    use a2ui_base::message_processor::MessageProcessor;
    use ratatui::backend::TestBackend;

    /// Build a surface whose `root` is described by `components_json` (an array of
    /// component objects), then render it into a fresh `cols x rows` TestBackend
    /// buffer and return the buffer.
    fn render_to_buffer(
        components_json: serde_json::Value,
        cols: u16,
        rows: u16,
    ) -> ratatui::buffer::Buffer {
        let registry = build_basic_registry();
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "test",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "dataModel": {}
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();
        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": { "surfaceId": "test", "components": components_json }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();

        let surface = processor.model.get_surface("test").expect("surface exists");
        let backend = TestBackend::new(cols, rows);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let render_catalog = Catalog::new("placeholder");
        terminal
            .draw(|frame| {
                let renderer = SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, frame.area(), None);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    /// Like [`render_to_buffer`], but passes a `focused_id` so focus-driven
    /// styling (e.g. a TextField's yellow border) can be asserted in tests.
    fn render_to_buffer_focused(
        components_json: serde_json::Value,
        cols: u16,
        rows: u16,
        focused_id: Option<&str>,
    ) -> ratatui::buffer::Buffer {
        let registry = build_basic_registry();
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "test",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "dataModel": {}
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();
        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": { "surfaceId": "test", "components": components_json }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();
        let surface = processor.model.get_surface("test").expect("surface exists");
        let backend = TestBackend::new(cols, rows);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let render_catalog = Catalog::new("placeholder");
        terminal
            .draw(|frame| {
                let renderer = SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, frame.area(), focused_id);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    /// Like [`render_to_buffer`], but renders via `SurfaceRenderer::render_scrolled`
    /// into a `cols x rows` viewport with the given `scroll_offset`. Used to test
    /// that scrolling reveals a true slice of the natural-height layout.
    fn render_scrolled_to_buffer(
        components_json: serde_json::Value,
        cols: u16,
        rows: u16,
        scroll_offset: usize,
    ) -> ratatui::buffer::Buffer {
        let registry = build_basic_registry();
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "test",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "dataModel": {}
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();
        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": { "surfaceId": "test", "components": components_json }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();
        let surface = processor.model.get_surface("test").expect("surface exists");
        let backend = TestBackend::new(cols, rows);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let render_catalog = Catalog::new("placeholder");
        terminal
            .draw(|frame| {
                let renderer = SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render_scrolled(frame, frame.area(), scroll_offset, None);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    /// Render `buf` as a flat screen string (one char per cell, rows joined by `\n`).
    fn screen_string(buf: &ratatui::buffer::Buffer, cols: u16, rows: u16) -> String {
        let mut s = String::new();
        for y in 0..rows {
            for x in 0..cols {
                s.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            s.push('\n');
        }
        s
    }

    /// True if `buf`'s top `rows` rows exactly match `reference`'s rows
    /// `[ref_y0, ref_y0 + rows)` — a cell-for-cell slice comparison.
    fn viewport_matches_reference(
        buf: &ratatui::buffer::Buffer,
        reference: &ratatui::buffer::Buffer,
        ref_y0: u16,
        rows: u16,
        cols: u16,
    ) -> bool {
        for r in 0..rows {
            for c in 0..cols {
                if buf[(c, r)] != reference[(c, ref_y0 + r)] {
                    return false;
                }
            }
        }
        true
    }

    /// True if every cell in row `y` (across `width` columns) is a blank/space.
    fn row_is_blank(buf: &ratatui::buffer::Buffer, y: u16, width: u16) -> bool {
        (0..width).all(|x| buf[(x, y)].symbol() == " ")
    }

    /// Build a surface whose `root` is described by `components_json` and return
    /// its measured natural height via the public `SurfaceRenderer::measure` API.
    fn measure_root(components_json: serde_json::Value, width: u16) -> Option<u16> {
        let registry = build_basic_registry();
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "test",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
                "dataModel": {}
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();
        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": { "surfaceId": "test", "components": components_json }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();

        let surface = processor.model.get_surface("test").expect("surface exists");
        let render_catalog = Catalog::new("placeholder");
        SurfaceRenderer::new(surface, &registry, &render_catalog).measure(width)
    }

    /// Count rows that contain any non-blank content.
    fn non_blank_row_count(buf: &ratatui::buffer::Buffer, cols: u16, rows: u16) -> u16 {
        (0..rows).filter(|&y| !row_is_blank(buf, y, cols)).count() as u16
    }

    #[test]
    fn card_root_does_not_fill_screen() {
        // Card > Column > [Text, Text]. Natural height = 6 (two texts) + 4 (card chrome) = 10.
        // In a 24-tall area it shrink-wraps to ~10 and centers → top and bottom edges blank.
        let components = serde_json::json!([
            { "id": "root", "component": "Card", "child": "inner" },
            { "id": "inner", "component": "Column", "children": ["a", "b"] },
            { "id": "a", "component": "Text", "text": "Title" },
            { "id": "b", "component": "Text", "text": "Body" }
        ]);
        let buf = render_to_buffer(components, 40, 24);

        // Before the measure pass the Card filled all 24 rows; now the top and bottom
        // edge rows must be blank (card is centered & content-sized).
        assert!(
            row_is_blank(&buf, 0, 40),
            "top edge should be blank — card shrink-wrapped"
        );
        assert!(
            row_is_blank(&buf, 23, 40),
            "bottom edge should be blank — card shrink-wrapped"
        );
        // And the content occupies only a fraction of the screen.
        let used = non_blank_row_count(&buf, 40, 24);
        assert!(
            used <= 12,
            "card content should occupy <=12 rows, used {used}"
        );
    }

    #[test]
    fn measure_card_root_returns_natural_height() {
        // Card > Column > [Text, Text]. Each Text = 1 content line + 2 margin = 3;
        // Column = 3 + 3 = 6; Card adds 4 chrome → natural height 10.
        let components = serde_json::json!([
            { "id": "root", "component": "Card", "child": "inner" },
            { "id": "inner", "component": "Column", "children": ["a", "b"] },
            { "id": "a", "component": "Text", "text": "Title" },
            { "id": "b", "component": "Text", "text": "Body" }
        ]);
        assert_eq!(
            measure_root(components, 40),
            Some(10),
            "Card>Column>[Text,Text] natural height = 6 content + 4 chrome"
        );
    }

    #[test]
    fn measure_column_root_sums_children() {
        // Column > [Text × 3] → 3 + 3 + 3 = 9.
        let components = serde_json::json!([
            { "id": "root", "component": "Column", "children": ["a", "b", "c"] },
            { "id": "a", "component": "Text", "text": "One" },
            { "id": "b", "component": "Text", "text": "Two" },
            { "id": "c", "component": "Text", "text": "Three" }
        ]);
        assert_eq!(measure_root(components, 40), Some(9));
    }

    #[test]
    fn measure_text_wraps_with_width() {
        // A single long Text line wraps across more rows at narrow width and fewer
        // at wide width — proving measure is width-aware (the streaming-text fix).
        let components = serde_json::json!([
            { "id": "root", "component": "Text", "text": "alpha beta gamma delta epsilon zeta eta theta" }
        ]);
        let narrow = measure_root(components.clone(), 12).expect("narrow measured");
        let wide = measure_root(components, 60).expect("wide measured");
        assert!(
            narrow > wide,
            "narrow width should wrap to more rows than wide: narrow={narrow} wide={wide}"
        );
        assert!(wide >= 3, "wide text still has the +2 margin floor");
    }

    #[test]
    fn focused_textfield_border_is_colored_only_when_focused() {
        // Two TextFields; focusing the first must color its border (the
        // component paints a yellow border when ctx.focused_id matches), while
        // passing no focus paints nothing yellow. This is the invariant 04_login_form
        // violated by passing `None` to SurfaceRenderer::render.
        use ratatui::style::Color;
        let components = serde_json::json!([
            { "id": "root", "component": "Column", "children": ["user", "pass"] },
            { "id": "user", "component": "TextField", "label": "User", "value": {"path":"/u"} },
            { "id": "pass", "component": "TextField", "label": "Pass", "value": {"path":"/p"} }
        ]);
        let any_yellow = |buf: &ratatui::buffer::Buffer| {
            (0..24u16).any(|y| (0..40u16).any(|x| buf[(x, y)].fg == Color::Yellow))
        };
        let focused = render_to_buffer_focused(components.clone(), 40, 24, Some("user"));
        assert!(
            any_yellow(&focused),
            "focused TextField should paint a yellow border"
        );

        let plain = render_to_buffer_focused(components, 40, 24, None);
        assert!(
            !any_yellow(&plain),
            "no focus passed → no yellow highlight anywhere"
        );
    }

    #[test]
    fn textfield_in_column_renders_a_proper_box() {
        // Column > [TextField]. The TextField draws a margin + a bordered block, so it
        // needs ≥5 rows to show its top border, content, and bottom border.
        // Before the height fix it collapsed to 1-2 lines (border only); now it must
        // render a real 3-line box (top border / content / bottom border).
        let components = serde_json::json!([
            { "id": "root", "component": "Column", "children": ["field"] },
            { "id": "field", "component": "TextField", "label": "Username", "value": "alice" }
        ]);
        let buf = render_to_buffer(components, 40, 24);

        let used = non_blank_row_count(&buf, 40, 24);
        assert!(
            (3..=6).contains(&used),
            "TextField should render a ~3-line box (border/content/border), used {used} rows"
        );
        // The box must show both a top and bottom horizontal border (`─`).
        let border_rows: Vec<u16> = (0..24)
            .filter(|&y| (0..40).any(|x| buf[(x, y)].symbol() == "─"))
            .collect();
        assert!(
            border_rows.len() >= 2,
            "TextField box should have ≥2 border rows, found {border_rows:?}"
        );
    }

    #[test]
    fn column_root_fills_viewport_vertically() {
        // A Column root fills the viewport (unlike a Card root). With justify=stretch,
        // two Text children are spread across the full height: one near the top, one
        // near the bottom — proving the column did not compact to the center.
        let components = serde_json::json!([
            { "id": "root", "component": "Column", "children": ["top", "bottom"], "justify": "stretch" },
            { "id": "top", "component": "Text", "text": "TOP" },
            { "id": "bottom", "component": "Text", "text": "BOTTOM" }
        ]);
        let buf = render_to_buffer(components, 40, 24);

        let top_filled = (0..6u16).any(|y| !row_is_blank(&buf, y, 40));
        let bottom_filled = (12..24u16).any(|y| !row_is_blank(&buf, y, 40));
        assert!(
            top_filled,
            "first child should render near the top of a filling column"
        );
        assert!(
            bottom_filled,
            "second child should render near the bottom of a filling column"
        );
    }

    #[test]
    fn login_form_inputs_render_as_full_boxes() {
        // Mirrors examples/04_login_form.rs: Card > Column > [Text, TextField, TextField,
        // Button]. Before the height fix the inputs collapsed to 1-2 lines; now each
        // bordered input/button must render a real box (top + bottom border rows).
        let components = serde_json::json!([
            { "id": "root", "component": "Card", "child": "form" },
            { "id": "form", "component": "Column", "children": ["title", "user", "pass", "submit"] },
            { "id": "title", "component": "Text", "text": "Welcome Back" },
            { "id": "user", "component": "TextField", "label": "Username", "value": "" },
            { "id": "pass", "component": "TextField", "label": "Password", "value": "" },
            { "id": "submit", "component": "Button", "child": "submit_label" },
            { "id": "submit_label", "component": "Text", "text": "Sign In" }
        ]);
        let buf = render_to_buffer(components, 80, 24);

        // The Button's child text "Sign In" must be visible — it only renders when the
        // Button gets enough height (≥5) to show border + content. Before the
        // nested-margin fix the label vanished.
        let mut screen = String::new();
        for y in 0..24u16 {
            for x in 0..80u16 {
                screen.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
        }
        assert!(
            screen.contains("Sign In"),
            "Button label 'Sign In' should render"
        );

        let border_rows = (0..24u16)
            .filter(|&y| (0..80u16).any(|x| buf[(x, y)].symbol() == "─"))
            .count();
        assert!(
            border_rows >= 8,
            "2 TextFields + Button + Card ⇒ ≥8 border rows, found {border_rows}"
        );
    }

    #[test]
    fn templated_children_expand_from_data_array() {
        // Mirrors the "Incremental List" sample (minimal catalog): a root Column
        // whose `children` is a template `{path, componentId}` bound to a data
        // array. Each array element must instantiate the template component with
        // its own nested data path.
        //
        // Regression: the `componentId` (camelCase) key did not deserialize into
        // `ChildList::Template` (snake_case `component_id` with no serde rename),
        // so `children()` returned `None` and the Column rendered a blank panel.
        use crate::catalogs::minimal::{build_minimal_catalog, build_minimal_registry};

        let registry = build_minimal_registry();
        let mut processor = MessageProcessor::new(vec![build_minimal_catalog()]);

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "example_7",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json"
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        let set_data = serde_json::json!({
            "version": "v1.0",
            "updateDataModel": {
                "surfaceId": "example_7",
                "path": "/",
                "value": { "restaurants": [
                    { "title": "The Golden Fork", "subtitle": "Fine Dining & Spirits", "address": "123 Gastronomy Lane" },
                    { "title": "Ocean's Bounty", "subtitle": "Fresh Daily Seafood", "address": "456 Shoreline Dr" }
                ] }
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&set_data.to_string()).unwrap())
            .unwrap();

        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": {
                "surfaceId": "example_7",
                "components": [
                    { "id": "root", "component": "Column", "children": { "path": "/restaurants", "componentId": "restaurant_card" } },
                    { "id": "restaurant_card", "component": "Column", "children": ["rc_title", "rc_subtitle", "rc_address"] },
                    { "id": "rc_title", "component": "Text", "text": { "path": "title" } },
                    { "id": "rc_subtitle", "component": "Text", "text": { "path": "subtitle" } },
                    { "id": "rc_address", "component": "Text", "text": { "path": "address" } }
                ]
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();

        // Confirm the children parsed as a Template, not None.
        let surface = processor
            .model
            .get_surface("example_7")
            .expect("surface exists");
        {
            let components = surface.components.borrow();
            let root = components.get("root").expect("root exists");
            match root.children() {
                Some(a2ui_base::protocol::common_types::ChildList::Template {
                    component_id,
                    path,
                }) => {
                    assert_eq!(component_id, "restaurant_card");
                    assert_eq!(path, "/restaurants");
                }
                other => panic!("root.children should be Template, got {other:?}"),
            }
        }

        let backend = TestBackend::new(60, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let render_catalog = Catalog::new("placeholder");
        terminal
            .draw(|frame| {
                let renderer = SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, frame.area(), None);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let mut screen = String::new();
        for y in 0..24u16 {
            for x in 0..60u16 {
                screen.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            screen.push('\n');
        }

        assert!(
            screen.contains("The Golden Fork"),
            "first restaurant title should render:\n{screen}"
        );
        assert!(
            screen.contains("Ocean's Bounty"),
            "second restaurant title should render:\n{screen}"
        );
        assert!(
            screen.contains("Fine Dining & Spirits"),
            "first restaurant subtitle should render:\n{screen}"
        );
        assert!(
            screen.contains("456 Shoreline Dr"),
            "second restaurant address should render:\n{screen}"
        );
    }

    #[test]
    fn at_index_system_function_renders_in_template_items() {
        // The `@index` system function (common_types.json → indexSystemFunction)
        // must resolve to each template item's 0-based index. With offset:1 a
        // 3-item list renders "1", "2", "3" — proving the index is derived from
        // the item's base path with zero backend-specific plumbing. If @index
        // were unimplemented, every item would show "" or a constant "1".
        use crate::catalogs::minimal::{build_minimal_catalog, build_minimal_registry};

        let registry = build_minimal_registry();
        let mut processor = MessageProcessor::new(vec![build_minimal_catalog()]);

        let create = serde_json::json!({
            "version": "v1.0",
            "createSurface": {
                "surfaceId": "idx_surf",
                "catalogId": "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json"
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
            .unwrap();

        let set_data = serde_json::json!({
            "version": "v1.0",
            "updateDataModel": {
                "surfaceId": "idx_surf",
                "path": "/",
                "value": { "items": [
                    { "label": "Apple" },
                    { "label": "Banana" },
                    { "label": "Cherry" }
                ] }
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&set_data.to_string()).unwrap())
            .unwrap();

        let update = serde_json::json!({
            "version": "v1.0",
            "updateComponents": {
                "surfaceId": "idx_surf",
                "components": [
                    { "id": "root", "component": "Column", "children": { "path": "/items", "componentId": "item" } },
                    { "id": "item", "component": "Row", "children": ["idx", "name"] },
                    { "id": "idx", "component": "Text", "text": { "call": "@index", "args": { "offset": 1 } } },
                    { "id": "name", "component": "Text", "text": { "path": "label" } }
                ]
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();

        let surface = processor
            .model
            .get_surface("idx_surf")
            .expect("surface exists");
        let backend = TestBackend::new(60, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let render_catalog = Catalog::new("placeholder");
        terminal
            .draw(|frame| {
                let renderer = SurfaceRenderer::new(surface, &registry, &render_catalog);
                renderer.render(frame, frame.area(), None);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let mut screen = String::new();
        for y in 0..24u16 {
            for x in 0..60u16 {
                screen.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            screen.push('\n');
        }

        // Template expansion + @index: the three labels render, and the index
        // text yields 1, 2, 3 (offset 1 over a 0-based 0,1,2).
        assert!(screen.contains("Apple"), "item 0 label rendered:\n{screen}");
        assert!(
            screen.contains("Banana"),
            "item 1 label rendered:\n{screen}"
        );
        assert!(
            screen.contains("Cherry"),
            "item 2 label rendered:\n{screen}"
        );
        assert!(
            screen.contains('1') && screen.contains('2') && screen.contains('3'),
            "@index must render 1,2,3 across the three template items:\n{screen}"
        );
    }

    // -------------------------------------------------------------------------
    // render_scrolled: scrollable-viewport rendering (no squish, true slices).
    // -------------------------------------------------------------------------

    /// 6 Text children → each natural height 3 → column natural height 18.
    fn scrolled_column_components() -> serde_json::Value {
        serde_json::json!([
            { "id": "root", "component": "Column", "children": ["t0", "t1", "t2", "t3", "t4", "t5"] },
            { "id": "t0", "component": "Text", "text": "ROW 0" },
            { "id": "t1", "component": "Text", "text": "ROW 1" },
            { "id": "t2", "component": "Text", "text": "ROW 2" },
            { "id": "t3", "component": "Text", "text": "ROW 3" },
            { "id": "t4", "component": "Text", "text": "ROW 4" },
            { "id": "t5", "component": "Text", "text": "ROW 5" }
        ])
    }

    #[test]
    fn render_scrolled_top_slice_is_unsquished_reference_slice() {
        // Reference: the full natural-height render (40 wide × 18 tall).
        let comps = scrolled_column_components();
        let reference = render_to_buffer(comps.clone(), 40, 18);

        // A 6-row viewport scrolled to the top must be exactly the reference's
        // top 6 rows — proving the layout is rendered at natural height and only
        // sliced, NOT reflowed/compressed into the 6-row viewport.
        let top = render_scrolled_to_buffer(comps, 40, 6, 0);
        assert!(
            viewport_matches_reference(&top, &reference, 0, 6, 40),
            "top viewport must equal the reference's top 6 rows (no squish)"
        );
        let screen = screen_string(&top, 40, 6);
        assert!(
            screen.contains("ROW 0"),
            "top slice shows the first row:\n{screen}"
        );
        assert!(
            !screen.contains("ROW 5"),
            "top slice must not show the last row:\n{screen}"
        );
    }

    #[test]
    fn render_scrolled_bottom_slice_matches_reference() {
        let comps = scrolled_column_components();
        let reference = render_to_buffer(comps.clone(), 40, 18);

        // Offset 12 of an 18-tall surface → the bottom 6 rows.
        let bottom = render_scrolled_to_buffer(comps, 40, 6, 12);
        assert!(
            viewport_matches_reference(&bottom, &reference, 12, 6, 40),
            "bottom viewport must equal the reference's rows 12..18"
        );
        let screen = screen_string(&bottom, 40, 6);
        assert!(
            screen.contains("ROW 5"),
            "bottom slice shows the last row:\n{screen}"
        );
        assert!(
            !screen.contains("ROW 0"),
            "bottom slice must not show the first row:\n{screen}"
        );
    }

    #[test]
    fn render_scrolled_middle_slice_matches_reference() {
        // A slice that is clipped on BOTH sides (top scrolled off, bottom not yet
        // reached) — the case that needed negative-y handling before.
        let comps = scrolled_column_components();
        let reference = render_to_buffer(comps.clone(), 40, 18);

        let mid = render_scrolled_to_buffer(comps, 40, 4, 6);
        assert!(
            viewport_matches_reference(&mid, &reference, 6, 4, 40),
            "middle viewport must equal the reference's rows 6..10"
        );
        let screen = screen_string(&mid, 40, 4);
        assert!(
            screen.contains("ROW 2"),
            "middle slice shows ROW 2:\n{screen}"
        );
        assert!(
            !screen.contains("ROW 0") && !screen.contains("ROW 5"),
            "middle slice shows neither end:\n{screen}"
        );
    }

    #[test]
    fn render_scrolled_does_not_compress_tall_content_into_tiny_viewport() {
        // The regression: a tall surface (18 rows) in a 2-row viewport must still
        // show the *top* two natural rows, not all six ROWs mashed into 2 lines.
        let comps = scrolled_column_components();
        let reference = render_to_buffer(comps.clone(), 40, 18);

        let tiny = render_scrolled_to_buffer(comps, 40, 2, 0);
        assert!(
            viewport_matches_reference(&tiny, &reference, 0, 2, 40),
            "2-row viewport must be the reference's top 2 rows — sliced, not compressed"
        );
        let screen = screen_string(&tiny, 40, 2);
        assert!(
            screen.contains("ROW 0"),
            "tiny viewport still shows ROW 0:\n{screen}"
        );
        assert!(
            !screen.contains("ROW 5"),
            "last row must not be compressed into the 2-row viewport:\n{screen}"
        );
    }

    #[test]
    fn render_scrolled_offset_past_end_is_blank() {
        // Scrolling past the surface's end renders nothing visible.
        let comps = scrolled_column_components();
        let past = render_scrolled_to_buffer(comps, 40, 6, 100);
        for y in 0..6u16 {
            assert!(
                row_is_blank(&past, y, 40),
                "row {y} should be blank past end"
            );
        }
    }
}
