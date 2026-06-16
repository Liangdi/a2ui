//! Surface renderer — entry point for rendering an A2UI component tree into a ratatui frame.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    widgets::{Block, Clear, Paragraph},
};

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::catalog::Catalog;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::model::surface_model::SurfaceModel;
use a2ui_base::protocol::common_types::DynamicBoolean;
use super::component_impl::ComponentRegistry;
use super::component_impl::TuiComponent;

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
    pub fn render(&self, frame: &mut Frame, area: Rect, focused_id: Option<&str>) {
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

        // Any open Modal floats its content as a centered overlay on top of the
        // rendered tree — a real modal dialog rather than an inline swap.
        self.render_modal_overlays(frame, area, surface_id, &data_model, &components, focused_id);
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
        for (_, m) in components.all() {
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
            frame.render_widget(Block::default().style(Style::default().bg(Color::Black)), area);

            // Centered dialog box.
            let dialog = centered_rect(area, 60, 40);
            frame.render_widget(Clear, dialog);
            frame.render_widget(
                Block::bordered().title(" Modal ").style(Style::default().bg(Color::Gray)),
                dialog,
            );
            let inner = dialog.inner(Margin { horizontal: 1, vertical: 1 });
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
    let mut render_child = |child_id: &str, child_area: Rect, child_frame: &mut Frame, child_base_path: &str| {
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
    let mut measure_child = |child_id: &str, child_base_path: &str, available_width: u16| -> Option<u16> {
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

    let tui_comp = match registry.get(&comp_model.component_type) {
        Some(c) => c,
        None => return None,
    };

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
    use a2ui_base::message_processor::MessageProcessor;
    use crate::catalogs::basic::{build_basic_catalog, build_basic_registry};
    use ratatui::backend::TestBackend;

    /// Build a surface whose `root` is described by `components_json` (an array of
    /// component objects), then render it into a fresh `cols x rows` TestBackend
    /// buffer and return the buffer.
    fn render_to_buffer(components_json: serde_json::Value, cols: u16, rows: u16) -> ratatui::buffer::Buffer {
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
        assert!(row_is_blank(&buf, 0, 40), "top edge should be blank — card shrink-wrapped");
        assert!(row_is_blank(&buf, 23, 40), "bottom edge should be blank — card shrink-wrapped");
        // And the content occupies only a fraction of the screen.
        let used = non_blank_row_count(&buf, 40, 24);
        assert!(used <= 12, "card content should occupy <=12 rows, used {used}");
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
        assert!(any_yellow(&focused), "focused TextField should paint a yellow border");

        let plain = render_to_buffer_focused(components, 40, 24, None);
        assert!(!any_yellow(&plain), "no focus passed → no yellow highlight anywhere");
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
        assert!(top_filled, "first child should render near the top of a filling column");
        assert!(bottom_filled, "second child should render near the bottom of a filling column");
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
        assert!(screen.contains("Sign In"), "Button label 'Sign In' should render");

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
        let surface = processor.model.get_surface("example_7").expect("surface exists");
        {
            let components = surface.components.borrow();
            let root = components.get("root").expect("root exists");
            match root.children() {
                Some(a2ui_base::protocol::common_types::ChildList::Template { component_id, path }) => {
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

        assert!(screen.contains("The Golden Fork"), "first restaurant title should render:\n{screen}");
        assert!(screen.contains("Ocean's Bounty"), "second restaurant title should render:\n{screen}");
        assert!(screen.contains("Fine Dining & Spirits"), "first restaurant subtitle should render:\n{screen}");
        assert!(screen.contains("456 Shoreline Dr"), "second restaurant address should render:\n{screen}");
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

        let surface = processor.model.get_surface("idx_surf").expect("surface exists");
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
        assert!(screen.contains("Banana"), "item 1 label rendered:\n{screen}");
        assert!(screen.contains("Cherry"), "item 2 label rendered:\n{screen}");
        assert!(
            screen.contains('1') && screen.contains('2') && screen.contains('3'),
            "@index must render 1,2,3 across the three template items:\n{screen}"
        );
    }
}
