//! Text component — renders a styled paragraph.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::component_impl::TuiComponent;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::DynamicString;

/// Text component implementation.
///
/// Renders a `ratatui::widgets::Paragraph` with variant-based styling.
/// Applies a default margin of 1 cell on all sides (leaf component).
pub struct TextComponent;

impl TuiComponent for TextComponent {
    fn name(&self) -> &'static str {
        "Text"
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

        // Resolve the text content.
        let text_content = match comp_model.get_property::<DynamicString>("text") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Determine variant styling.
        let variant: Option<String> = comp_model.get_property("variant");
        let style = match variant.as_deref() {
            Some("h1") => Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(ratatui::style::Color::Cyan),
            Some("h2") => Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(ratatui::style::Color::Green),
            Some("h3") => Style::default().add_modifier(Modifier::BOLD),
            Some("h4") => Style::default().add_modifier(Modifier::UNDERLINED),
            Some("h5") => Style::default().add_modifier(Modifier::ITALIC),
            Some("caption") => Style::default().add_modifier(Modifier::DIM),
            Some("body") | None => Style::default(),
            _ => Style::default(),
        };

        // Apply default margin of 1 cell on all sides, but never collapse to zero so a
        // Text nested in a tight area (e.g. a Button label) still renders.
        let inner = crate::layout_engine::padded_content(area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let paragraph = Paragraph::new(text_content)
            .style(style)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    fn natural_height(
        &self,
        ctx: &ComponentContext,
        available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id);

        // Resolve the text content (None → empty string).
        let content = match comp_model.and_then(|m| m.get_property::<DynamicString>("text")) {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Mirror `padded_content`: subtract 1 cell per side (→ 2 total) only when
        // the available width is > 2; otherwise the render pass keeps the full width.
        let content_width = if available_width > 2 {
            available_width.saturating_sub(2)
        } else {
            available_width
        };
        // Avoid division-by-zero; ratatui can't render anything in 0 cols anyway.
        let content_width = content_width.max(1) as usize;

        let mut total: usize = 0;
        for line in content.split('\n') {
            total += wrapped_row_count(line, content_width);
        }

        // +2 for the margin the render adds via `padded_content` (1 cell top + 1 bottom),
        // matching the horizontal subtraction above.
        Some((total as u16).saturating_add(2))
    }
}

/// Count how many display rows `line` occupies when wrapped at `content_width`
/// using a greedy word-wrap that mirrors ratatui's `Wrap { trim: false }`.
///
/// Rules (matching ratatui 0.30 behaviour for `trim: false`):
/// - Words are split on ASCII spaces (`' '`). Consecutive spaces are preserved
///   (trim:false keeps leading/repeated whitespace).
/// - A single separating space (width 1) is kept between words on the same row.
/// - When the next word (+ a preceding space when the line is non-empty) no
///   longer fits, it starts a new row.
/// - A word wider than `content_width` is broken greedily at `content_width`
///   boundaries (ratatui breaks long words), and any leftover width carries
///   over to continue the row.
/// - An empty line counts as 1 row.
fn wrapped_row_count(line: &str, content_width: usize) -> usize {
    // Split keeping the words; multiple consecutive spaces become empty "words"
    // but each space between real words still consumes width in the line. We
    // rebuild by iterating over the original spacing using split(' ') which
    // yields empty strings for consecutive delimiters — those empty strings are
    // treated as width-1 space separators (trim:false keeps them).
    if line.is_empty() {
        return 1;
    }

    let mut rows: usize = 0;
    let mut line_width: usize = 0; // current row's used width
    let mut started = false; // has any content been placed on the current row?

    let push_sep = |w: &mut usize, started: &mut bool| {
        // A separator space only counts when continuing a row that already has a word.
        if *started {
            *w += 1;
        }
    };

    // Iterate words split on ' '. Track gaps via the empty-string fragments that
    // split(' ') emits for consecutive delimiters.
    for word in line.split(' ') {
        let word_w = UnicodeWidthStr::width(word);

        if word.is_empty() {
            // A bare separator space (consecutive delimiters, or leading space).
            // It belongs to the current row: add its width; if it overflows, wrap.
            push_sep(&mut line_width, &mut started);
            // The separator itself is 1 cell; if the row now exceeds width, it
            // forces a wrap. With trim:false ratatui keeps the space on the
            // current row (it does not move it down), so just mark started.
            started = true;
            continue;
        }

        // Tentative width if we append this word (with a separating space when
        // the current row already has content).
        let sep = if started { 1 } else { 0 };
        if line_width + sep + word_w <= content_width {
            // Fits on the current row.
            line_width += sep + word_w;
            started = true;
            continue;
        }

        // Doesn't fit. Two sub-cases:
        // (a) The word itself fits within content_width → start a fresh row.
        // (b) The word is wider than content_width → it must be broken across rows.
        if word_w <= content_width {
            rows += 1; // close the previous row
            line_width = word_w;
            started = true;
            continue;
        }

        // Long word: break greedily.
        // First, if the current row has leftover space, ratatui fills it with as
        // much of the word as fits, then continues on new rows. We account for
        // the partial fill that fits in the remaining space (no separator here
        // since we're mid-word).
        let mut remaining = word_w;
        if started && line_width < content_width {
            // Fill the rest of the current row with the head of the word.
            let fits = content_width - line_width; // no separator: continuing the word
            // `fits` cells consumed from the word.
            let _ = fits; // consumed implicitly: subtract below
            // Consume `fits` worth of the word off the front.
            remaining = remaining.saturating_sub(content_width - line_width);
            rows += 1; // close this (now-full) row
            started = false;
            line_width = 0;
        }
        // Now break the rest of the word into full-width chunks.
        while remaining > 0 {
            if remaining > content_width {
                rows += 1;
                remaining -= content_width;
            } else {
                // Tail of the word fits on one row.
                line_width = remaining;
                started = true;
                remaining = 0;
            }
        }
    }

    // Account for the final (open) row.
    rows + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogs::basic::{build_basic_catalog, build_basic_registry};
    use crate::component_impl::TuiComponent;
    use crate::surface::SurfaceRenderer;
    use a2ui_base::catalog::Catalog;
    use a2ui_base::message_processor::MessageProcessor;
    use ratatui::backend::TestBackend;
    use std::collections::HashMap;

    /// Build a surface whose `root` is a single Text with the given `text`,
    /// render it into a `cols x rows` TestBackend buffer, and return the buffer
    /// plus the non-blank row count over the whole area.
    fn render_text_to_buffer(text: &str, cols: u16, rows: u16) -> ratatui::buffer::Buffer {
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
            "updateComponents": {
                "surfaceId": "test",
                "components": [
                    { "id": "root", "component": "Text", "text": text }
                ]
            }
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

    /// Count rows (within `rows` tall, scanning all `cols` columns) that contain
    /// any non-blank cell.
    fn non_blank_row_count(buf: &ratatui::buffer::Buffer, cols: u16, rows: u16) -> u16 {
        (0..rows)
            .filter(|&y| (0..cols).any(|x| buf[(x, y)].symbol() != " "))
            .count() as u16
    }

    /// Measure the Text root's natural height at a given available width by
    /// invoking `TextComponent.natural_height` directly with a no-op
    /// measure_child closure (Text is a leaf, ignores measure_child).
    fn measure_text(text: &str, available_width: u16) -> u16 {
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
            "updateComponents": {
                "surfaceId": "test",
                "components": [
                    { "id": "root", "component": "Text", "text": text }
                ]
            }
        });
        processor
            .process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
            .unwrap();

        let surface = processor.model.get_surface("test").expect("surface exists");
        let components = surface.components.borrow();
        let data_model = surface.data_model.borrow();
        let functions: HashMap<
            String,
            Box<dyn a2ui_base::catalog::function_api::FunctionImplementation>,
        > = HashMap::new();
        let ctx = ComponentContext::new(
            "root".to_string(),
            "test".to_string(),
            &data_model,
            &components,
            &functions,
            "",
            None,
        );

        let mut measure_child = |_id: &str, _base: &str, _w: u16| -> Option<u16> { None };
        TextComponent
            .natural_height(&ctx, available_width, &mut measure_child)
            .expect("natural_height returns Some")
    }

    /// The Text root is shrink-wrapped & vertically centered by the surface
    /// renderer, so the buffer it draws equals the natural height the renderer
    /// measured. `natural_height` is the FULL footprint (text rows + 1-cell
    /// margin top + 1-cell margin bottom = +2), while `non_blank_row_count`
    /// only counts rows that have content — the two margin rows are blank. So
    /// the locked invariant is: `measured == rendered_non_blank + 2`.
    fn assert_consistent(text: &str, cols: u16, rows: u16) {
        let buf = render_text_to_buffer(text, cols, rows);
        let rendered = non_blank_row_count(&buf, cols, rows);
        let measured = measure_text(text, cols);
        assert_eq!(
            measured,
            rendered + 2,
            "measure/render mismatch at {cols}x{rows}: natural_height returned {measured} (full \
             footprint), but rendered {rendered} non-blank content rows ⇒ footprint should be \
             {} (rows + 2 margin)\n\
             text={text:?}",
            rendered + 2,
        );
    }

    #[test]
    fn natural_height_matches_render_narrow() {
        // ~60 chars of words, wrapped at a narrow width (cols=20 ⇒ content_width=18).
        let text = "the quick brown fox jumps over the lazy dog and runs away fast";
        assert_consistent(text, 20, 30);
    }

    #[test]
    fn natural_height_matches_render_wide() {
        // Same string at a wider width (cols=40 ⇒ content_width=38).
        let text = "the quick brown fox jumps over the lazy dog and runs away fast";
        assert_consistent(text, 40, 30);
    }

    #[test]
    fn natural_height_matches_render_multiline() {
        // Explicit newlines plus a long final line that still wraps.
        let text = "first line\nsecond line here that is longer than the narrow width allows";
        assert_consistent(text, 20, 30);
        assert_consistent(text, 40, 20);
    }

    #[test]
    fn natural_height_matches_long_word() {
        // A single word wider than content_width forces word-breaking.
        let text = "supercalifragilisticexpialidocious is a very long word indeed";
        assert_consistent(text, 20, 30);
        assert_consistent(text, 16, 30);
    }
}
