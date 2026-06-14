//! Image component — renders an image in the TUI.
//!
//! Renders the image from a LOCAL file path via `ratatui-image`. The terminal
//! is probed **once** for the best native graphics protocol it supports and the
//! renderer degrades automatically: **kitty → iTerm2 → Sixel → Halfblocks**
//! (Halfblocks works in every terminal via colored half-block glyphs). If the
//! URL is not a loadable local path, or decoding fails, it falls back to the
//! text placeholder below. Use [`detected_protocol()`] to read back which
//! protocol actually won.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::DynamicString;
use crate::component_impl::TuiComponent;

/// Render the standard text placeholder into `inner`.
fn render_placeholder(
    variant_str: &str,
    content: &str,
    inner: Rect,
    frame: &mut Frame,
) {
    let placeholder = format!("[\u{1F5BC}{} {}]", variant_str, content);
    let paragraph = Paragraph::new(Line::from(Span::styled(
        placeholder,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(paragraph, inner);
}

/// Image component implementation.
///
/// Renders the real image via `ratatui-image` when the URL resolves to a
/// loadable local file path; otherwise shows the placeholder
/// `[🖼 description]`. Applies a default 1-cell margin.
pub struct ImageComponent;

impl TuiComponent for ImageComponent {
    fn name(&self) -> &'static str {
        "Image"
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
        let inner = crate::layout_engine::padded_content(area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Resolve description and URL.
        let description = match comp_model.get_property::<DynamicString>("description") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };
        let url = match comp_model.get_property::<DynamicString>("url") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve fit and variant properties.
        let _fit: Option<String> = comp_model.get_property("fit");
        let variant: Option<String> = comp_model.get_property("variant");
        let variant_str = variant.as_deref().map(|v| format!(" ({})", v)).unwrap_or_default();

        // Use description if available, otherwise fall back to URL.
        // (Borrow `url` rather than moving it so it remains available for the
        // real-render attempt below when the `image` feature is enabled.)
        let content = if !description.is_empty() {
            description
        } else if !url.is_empty() {
            url.clone()
        } else {
            "image".to_string()
        };

        // Attempt real rendering. On any failure (non-local URL, missing file,
        // decode error), fall back to the text placeholder so the render loop
        // never panics.
        if let Ok(()) = real::render(&url, inner, frame) {
            return;
        }

        render_placeholder(&variant_str, &content, inner, frame);
    }

    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // Placeholder is one line + 2 margin; real images scale to fit, so
        // authors grow them with `weight`.
        Some(3)
    }
}

// ---------------------------------------------------------------------------
// Real rendering via `ratatui-image`
// ---------------------------------------------------------------------------

mod real {
    use std::sync::OnceLock;

    use ratatui::{Frame, layout::{Rect, Size}};
    use ratatui_image::{
        Image, Resize,
        picker::{Picker, ProtocolType},
        protocol::Protocol,
    };

    /// The terminal-capability picker, cached for the process lifetime.
    ///
    /// `from_query_stdio` writes & reads stdio **once** to probe for the best
    /// native graphics protocol the terminal supports. On a "no capabilities"
    /// / "no response" result it already returns a Halfblocks picker; we
    /// additionally fall back to `halfblocks()` on any hard error. The result:
    /// the highest fidelity the terminal supports (kitty > iTerm2 > Sixel >
    /// Halfblocks), degrading automatically — no per-terminal config needed.
    ///
    /// The probe is blocking and re-queries the terminal, so it MUST be cached.
    /// Re-querying every render frame would stall the loop and flicker the
    /// screen. The first call lands during the first `draw()`, which is after
    /// the alternate screen is entered — exactly when ratatui-image requires.
    fn picker() -> &'static Picker {
        static PICKER: OnceLock<Picker> = OnceLock::new();
        PICKER.get_or_init(|| match Picker::from_query_stdio() {
            Ok(p) => p,
            Err(_) => Picker::halfblocks(),
        })
    }

    /// Human-readable name of the protocol the cached picker settled on.
    pub fn protocol_name() -> &'static str {
        match picker().protocol_type() {
            ProtocolType::Halfblocks => "Halfblocks",
            ProtocolType::Sixel => "Sixel",
            ProtocolType::Kitty => "Kitty",
            ProtocolType::Iterm2 => "iTerm2",
        }
    }

    /// Load the file at `path` as an image, build a protocol, and render it.
    ///
    /// Returns `Ok(())` only when the image was actually rendered; any failure
    /// (non-file URL, missing file, decode error, picker error) returns `Err`
    /// so the caller falls back to the placeholder.
    pub fn render(path: &str, inner: Rect, frame: &mut Frame) -> Result<(), ()> {
        // Only local file paths are supported — no HTTP fetch (would require a
        // heavy async/HTTP dep). Reject obviously non-path URLs early.
        if path.is_empty() || path.starts_with("http://") || path.starts_with("https://") {
            return Err(());
        }
        let path = std::path::Path::new(path);
        if !path.is_file() {
            return Err(());
        }

        let dyn_image = image::ImageReader::open(path)
            .map_err(|_| ())?
            .with_guessed_format()
            .map_err(|_| ())?
            .decode()
            .map_err(|_| ())?;

        // Use the cached terminal-capability picker: it picks the best native
        // protocol the terminal supports (kitty / iTerm2 / Sixel), degrading
        // to Halfblocks (works in any terminal) when none is available.
        let picker = picker();

        // v11 takes a `Size` (cell grid dimensions), not a `Rect`.
        let size = Size {
            width: inner.width,
            height: inner.height,
        };
        let protocol: Protocol = picker
            .new_protocol(dyn_image, size, Resize::Fit(None))
            .map_err(|_| ())?;

        // v11's `Image::new` borrows the protocol immutably.
        frame.render_widget(Image::new(&protocol), inner);
        Ok(())
    }
}

/// Name of the terminal graphics protocol the renderer settled on after probing
/// the terminal (e.g. `"Kitty"`, `"iTerm2"`, `"Sixel"`, or `"Halfblocks"`).
///
/// The probe runs at most once and is cached, so this is cheap to call per
/// frame — useful for showing the active protocol in a status bar.
pub fn detected_protocol() -> &'static str {
    real::protocol_name()
}
