//! Image component — renders an image in the TUI.
//!
//! Supports two image sources:
//! - **`data:` URIs** — `data:image/svg+xml,…` (rasterized via `resvg`) and
//!   `data:image/<png|jpeg|…>[;base64],…` (decoded via the `image` crate).
//! - **Local file paths** — raster formats via `image`, plus `.svg` via `resvg`.
//!
//! Remote `http(s)` URLs are NOT fetched (no async/HTTP dependency) and fall
//! back to the placeholder below, matching the other native backends.
//!
//! The terminal is probed **once** for the best native graphics protocol it
//! supports and the renderer degrades automatically: **kitty → iTerm2 → Sixel →
//! Halfblocks** (Halfblocks works in every terminal via colored half-block
//! glyphs). If the URL can't be decoded/loaded, it falls back to the text
//! placeholder. Use [`detected_protocol()`] to read back which protocol won.
//!
//! # Choosing the protocol
//!
//! Auto-detection can pick a protocol the terminal doesn't *actually* render —
//! e.g. a multiplexer (tmux/screen) that doesn't forward kitty/sixel graphics,
//! or a misreported `TERM`. Since there's no render-feedback channel:
//! - Under [`ImageProtocol::Auto`], we already play it safe inside a multiplexer
//!   (`TMUX`/`STY`/`ZELLIJ` set) by defaulting to Halfblocks, since the outer
//!   terminal's capability is usually reported but not forwarded.
//! - The protocol can be switched at **runtime** via [`set_image_protocol`]
//!   (what the gallery binds to a key and persists to its config file), or set
//!   as the initial baseline via the `A2UI_IMAGE_PROTOCOL` env var:
//!
//! | value | effect |
//! |---|---|
//! | `auto` (default) | probe the terminal, pick the best supported protocol |
//! | `kitty` / `iterm2` / `sixel` | force that native protocol |
//! | `halfblocks` | force the universal fallback (works in every terminal) |
//! | `none` | disable image rendering entirely (text placeholders only) |

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

/// Selectable image rendering protocol.
///
/// See the module docs for the full story. Switch at runtime with
/// [`set_image_protocol`]; read the active one back with
/// [`current_image_protocol`] / [`detected_protocol`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageProtocol {
    /// Probe the terminal and pick the best supported protocol; default to
    /// Halfblocks inside a multiplexer that won't forward native graphics.
    Auto,
    /// Kitty graphics protocol.
    Kitty,
    /// iTerm2 inline-image protocol.
    Iterm2,
    /// Sixel graphics protocol.
    Sixel,
    /// Universal fallback — colored half-block glyphs, works in every terminal.
    Halfblocks,
    /// Disable real image rendering; always show the text placeholder.
    None,
}

impl ImageProtocol {
    /// Canonical lowercase name — used for the `A2UI_IMAGE_PROTOCOL` env var and
    /// the gallery's persisted config file. Round-trips with [`from_name`].
    pub fn as_str(self) -> &'static str {
        match self {
            ImageProtocol::Auto => "auto",
            ImageProtocol::Kitty => "kitty",
            ImageProtocol::Iterm2 => "iterm2",
            ImageProtocol::Sixel => "sixel",
            ImageProtocol::Halfblocks => "halfblocks",
            ImageProtocol::None => "none",
        }
    }

    /// Parse a name (case-insensitive, with a few aliases: `half`, `iterm`,
    /// `off`, `text`). Unknown / empty → [`ImageProtocol::Auto`].
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "kitty" => ImageProtocol::Kitty,
            "iterm2" | "iterm" => ImageProtocol::Iterm2,
            "sixel" => ImageProtocol::Sixel,
            "halfblocks" | "half" => ImageProtocol::Halfblocks,
            "none" | "off" | "text" => ImageProtocol::None,
            _ => ImageProtocol::Auto, // "auto", empty, unknown
        }
    }
}

/// Set the image protocol at runtime (e.g. from a persisted config or a key
/// binding). Cheap: only records the choice — the terminal probe (font size)
/// is reused, so there is no re-probe. See [`ImageProtocol`].
pub fn set_image_protocol(protocol: ImageProtocol) {
    real::set_image_protocol(protocol);
}

/// The currently active [`ImageProtocol`] choice.
pub fn current_image_protocol() -> ImageProtocol {
    real::current_choice()
}

/// Human-readable name of the protocol in use (`"Kitty"`, `"Halfblocks"`, …) or
/// `"disabled"` when images are turned off ([`ImageProtocol::None`]).
pub fn detected_protocol() -> &'static str {
    real::protocol_name()
}

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
/// `data:` URI or loadable local file; otherwise shows the placeholder
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
    use std::sync::{OnceLock, RwLock};

    use ratatui::{Frame, layout::{Rect, Size}};
    use ratatui_image::{
        Image, Resize,
        picker::{Picker, ProtocolType},
        protocol::Protocol,
    };

    use super::ImageProtocol;

    /// The terminal-capability picker, probed **once** and cached for the
    /// process lifetime.
    ///
    /// `from_query_stdio` writes & reads stdio **once** to probe for the best
    /// native graphics protocol the terminal supports. On a "no capabilities"
    /// / "no response" result it already returns a Halfblocks picker; we
    /// additionally fall back to `halfblocks()` on any hard error.
    ///
    /// The probe is blocking and re-queries the terminal, so it MUST be cached
    /// and run after the alternate screen is entered. It is therefore lazy:
    /// first touched during the initial `draw()`. The cached picker holds the
    /// stable font size + tmux detection; the per-frame [`ProtocolType`] is
    /// applied to a cheap clone in [`render`] from the live [`ImageProtocol`]
    /// choice — so the protocol can be switched at runtime without re-probing.
    fn probed_picker() -> &'static Picker {
        static PICKER: OnceLock<Picker> = OnceLock::new();
        PICKER.get_or_init(|| match Picker::from_query_stdio() {
            Ok(p) => p,
            Err(_) => Picker::halfblocks(),
        })
    }

    /// The current [`ImageProtocol`] choice. Initialized lazily from
    /// `A2UI_IMAGE_PROTOCOL`; mutated at runtime via [`set_image_protocol`].
    /// Held in a `RwLock` so render (read) and a key-binding/config write never
    /// block each other for long.
    static CHOICE: OnceLock<RwLock<ImageProtocol>> = OnceLock::new();
    fn choice_lock() -> &'static RwLock<ImageProtocol> {
        CHOICE.get_or_init(|| RwLock::new(initial_choice()))
    }

    /// The initial choice from `A2UI_IMAGE_PROTOCOL` (the baseline before any
    /// runtime override). Parsed once.
    fn initial_choice() -> ImageProtocol {
        static ENV: OnceLock<ImageProtocol> = OnceLock::new();
        *ENV.get_or_init(|| parse_override(std::env::var("A2UI_IMAGE_PROTOCOL").ok().as_deref()))
    }

    /// Record a runtime protocol choice (no probe). Called by the gallery's
    /// key binding / config loader.
    pub fn set_image_protocol(choice: ImageProtocol) {
        *choice_lock().write().unwrap() = choice;
    }

    /// The currently active [`ImageProtocol`] choice.
    pub fn current_choice() -> ImageProtocol {
        *choice_lock().read().unwrap()
    }

    /// Resolve the choice to a concrete [`ProtocolType`], applying the
    /// multiplexer safety net under `Auto`. `None` ⇒ rendering is disabled.
    fn effective_type() -> Option<ProtocolType> {
        match current_choice() {
            ImageProtocol::None => None,
            ImageProtocol::Halfblocks => Some(ProtocolType::Halfblocks),
            ImageProtocol::Kitty => Some(ProtocolType::Kitty),
            ImageProtocol::Iterm2 => Some(ProtocolType::Iterm2),
            ImageProtocol::Sixel => Some(ProtocolType::Sixel),
            // ratatui-image's probe often reports the OUTER terminal's
            // capability, but a multiplexer won't actually forward the
            // graphics — so default to Halfblocks (works everywhere).
            ImageProtocol::Auto if in_multiplexer() => Some(ProtocolType::Halfblocks),
            ImageProtocol::Auto => Some(probed_picker().protocol_type()),
        }
    }

    /// Human-readable name of the protocol in use, or `"disabled"`.
    pub fn protocol_name() -> &'static str {
        match effective_type() {
            None => "disabled",
            Some(ProtocolType::Halfblocks) => "Halfblocks",
            Some(ProtocolType::Sixel) => "Sixel",
            Some(ProtocolType::Kitty) => "Kitty",
            Some(ProtocolType::Iterm2) => "iTerm2",
        }
    }

    /// Decode the image at `url`, build a graphics protocol, and render it.
    ///
    /// Returns `Ok(())` only when the image was actually rendered; any failure
    /// (unsupported scheme, missing file, decode/rasterize error, picker error,
    /// or [`ImageProtocol::None`]) returns `Err` so the caller shows the
    /// placeholder. Supported sources (see [`load_image`]): `data:` URIs (incl.
    /// SVG) and local file paths (incl. `.svg`). Remote `http(s)` not fetched.
    pub fn render(url: &str, inner: Rect, frame: &mut Frame) -> Result<(), ()> {
        let protocol_type = match effective_type() {
            None => return Err(()), // text-only mode → placeholder
            Some(p) => p,
        };
        let dyn_image = load_image(url)?;

        // Clone the cached picker and apply the live protocol type — the probe
        // (font size) is reused, only the encoder changes. The clone is dropped
        // at the end of the frame.
        let mut picker = probed_picker().clone();
        picker.set_protocol_type(protocol_type);

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

    /// Parse an `A2UI_IMAGE_PROTOCOL` value into an [`ImageProtocol`] (delegates
    /// to [`ImageProtocol::from_name`]; `None` ⇒ [`ImageProtocol::Auto`]).
    fn parse_override(value: Option<&str>) -> ImageProtocol {
        value.map(ImageProtocol::from_name).unwrap_or(ImageProtocol::Auto)
    }

    /// Whether the given multiplexer env-var values indicate we're inside one.
    /// Pure (no env access) so it can be unit-tested.
    fn multiplexer_detected(tmux: Option<&str>, sty: Option<&str>, zellij: Option<&str>) -> bool {
        [tmux, sty, zellij]
            .into_iter()
            .flatten()
            .any(|v| !v.is_empty())
    }

    /// Detect common terminal multiplexers (tmux, GNU screen, Zellij) that
    /// usually do NOT forward native graphics protocols (kitty/sixel/iTerm2)
    /// even when the outer terminal supports them.
    fn in_multiplexer() -> bool {
        fn present(var: &str) -> Option<String> {
            std::env::var_os(var).map(|v| v.to_string_lossy().into_owned())
        }
        multiplexer_detected(
            present("TMUX").as_deref(),
            present("STY").as_deref(),
            present("ZELLIJ").as_deref(),
        )
    }

    /// Decode a [`image::DynamicImage`] from a `data:` URI or a local file path.
    ///
    /// - `data:image/svg+xml[,;base64],…` → SVG rasterized via `resvg`
    ///   ([`rasterize_svg`]).
    /// - `data:image/<raster>[;base64],…` → decoded via the `image` crate.
    /// - local file: `.svg` → `resvg`; any other raster format → `image`.
    ///
    /// Remote `http(s)` URLs are rejected (no async/HTTP dependency) — they fall
    /// back to the text placeholder, matching the other native backends.
    fn load_image(url: &str) -> Result<image::DynamicImage, ()> {
        if let Some(rest) = url.strip_prefix("data:") {
            return load_data_uri(rest);
        }
        // Remote URLs are not fetched (no heavy async/HTTP dependency).
        if url.is_empty() || url.starts_with("http://") || url.starts_with("https://") {
            return Err(());
        }
        let path = std::path::Path::new(url);
        if !path.is_file() {
            return Err(());
        }

        // Local SVG → rasterize; every other format → the `image` crate.
        let is_svg = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
        if is_svg {
            let bytes = std::fs::read(url).map_err(|_| ())?;
            return rasterize_svg(&bytes);
        }

        image::ImageReader::open(url)
            .map_err(|_| ())?
            .with_guessed_format()
            .map_err(|_| ())?
            .decode()
            .map_err(|_| ())
    }

    /// Decode the body of a `data:` URI (the text after `data:`).
    ///
    /// Accepts both the `;base64` form (binary payloads, e.g. PNG/JPEG) and the
    /// URL-percent-encoded form (the common SVG case — e.g. `1.json`'s avatar).
    fn load_data_uri(rest: &str) -> Result<image::DynamicImage, ()> {
        let (metadata, data) = rest.split_once(',').ok_or(())?;
        let is_base64 = metadata.contains("base64");
        // MIME type is the first `;`-delimited segment, e.g. "image/svg+xml".
        let mime = metadata
            .split(';')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        let bytes: Vec<u8> = if is_base64 {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(data.trim())
                .map_err(|_| ())?
        } else {
            // Percent-decode into raw bytes (binary-safe; SVG bytes are UTF-8).
            percent_encoding::percent_decode_str(data).collect()
        };

        if mime == "image/svg+xml" {
            return rasterize_svg(&bytes);
        }
        image::load_from_memory(&bytes).map_err(|_| ())
    }

    /// Rasterize an SVG byte slice to a [`image::DynamicImage`] via `resvg`.
    ///
    /// Rendered at the SVG's intrinsic size, clamped to `[64, 1024]`px per side
    /// so neither tiny icons nor pathologically large documents blow up memory;
    /// SVGs without an intrinsic size fall back to 256×256. A white background
    /// is painted first so transparent SVGs stay legible on dark terminals.
    fn rasterize_svg(bytes: &[u8]) -> Result<image::DynamicImage, ()> {
        use resvg::{tiny_skia, usvg};

        let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).map_err(|_| ())?;

        let clamp = |v: f32| -> u32 {
            if !v.is_finite() || v <= 0.0 {
                256
            } else {
                v.round().clamp(64.0, 1024.0) as u32
            }
        };
        let size = tree.size();
        let width = clamp(size.width());
        let height = clamp(size.height());

        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(())?;
        pixmap.fill(tiny_skia::Color::WHITE);
        // Bind the mutable view so the borrow ends before `encode_png` below.
        let mut target = pixmap.as_mut();
        resvg::render(&tree, tiny_skia::Transform::default(), &mut target);

        let png = pixmap.encode_png().map_err(|_| ())?;
        image::load_from_memory(&png).map_err(|_| ())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The `a2ui-json/1.json` avatar shape: a URL-percent-encoded
        /// `data:image/svg+xml` URI. Must percent-decode → parse → rasterize to
        /// a real image with the SVG's intrinsic dimensions and content.
        #[test]
        fn svg_percent_encoded_data_uri_rasterizes() {
            // 100×80 SVG fully filled red (#ff0000).
            let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><rect width="100" height="80" fill="#ff0000"/></svg>"##;
            // Percent-encode exactly like `1.json`'s avatar (non-alphanumeric → %XX).
            let encoded = percent_encoding::utf8_percent_encode(svg, percent_encoding::NON_ALPHANUMERIC);
            let uri = format!("data:image/svg+xml,{encoded}");
            let img = load_data_uri(&uri["data:".len()..]).expect("svg data uri should rasterize");
            assert_eq!(img.width(), 100);
            assert_eq!(img.height(), 80);
            // The rect fills the whole canvas red — the center pixel must be red,
            // not the white background painted for transparent SVGs.
            let rgba = img.to_rgba8();
            let px = rgba.get_pixel(50, 40);
            assert_eq!(px.0, [255, 0, 0, 255], "center pixel should be opaque red");
        }

        /// The `;base64` variant of an SVG data URI must also rasterize.
        #[test]
        fn svg_base64_data_uri_rasterizes() {
            use base64::Engine;
            let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect width="96" height="96" fill="#00ff00"/></svg>"##;
            let b64 = base64::engine::general_purpose::STANDARD.encode(svg);
            let uri = format!("data:image/svg+xml;base64,{b64}");
            let img = load_data_uri(&uri["data:".len()..]).expect("base64 svg should rasterize");
            assert_eq!((img.width(), img.height()), (96, 96));
        }

        /// Remote / empty URLs are not fetched → `Err` (caller shows placeholder).
        #[test]
        fn remote_url_is_rejected() {
            assert!(load_image("https://example.com/a.png").is_err());
            assert!(load_image("http://example.com/a.png").is_err());
            assert!(load_image("").is_err());
        }

        /// `A2UI_IMAGE_PROTOCOL` is parsed case-insensitively; unknown/unset
        /// values fall back to `Auto` (probe the terminal).
        #[test]
        fn protocol_override_parsing() {
            assert_eq!(parse_override(None), ImageProtocol::Auto);
            assert_eq!(parse_override(Some("")), ImageProtocol::Auto);
            assert_eq!(parse_override(Some("auto")), ImageProtocol::Auto);
            assert_eq!(parse_override(Some("garbage")), ImageProtocol::Auto);

            assert_eq!(parse_override(Some("kitty")), ImageProtocol::Kitty);
            assert_eq!(parse_override(Some("iTerm2")), ImageProtocol::Iterm2);
            assert_eq!(parse_override(Some("sixel")), ImageProtocol::Sixel);
            assert_eq!(parse_override(Some("halfblocks")), ImageProtocol::Halfblocks);
            assert_eq!(parse_override(Some("half")), ImageProtocol::Halfblocks);

            // The three "disable image rendering" aliases.
            assert_eq!(parse_override(Some("none")), ImageProtocol::None);
            assert_eq!(parse_override(Some("off")), ImageProtocol::None);
            assert_eq!(parse_override(Some("TEXT")), ImageProtocol::None);
        }

        /// Multiplexer detection: any non-empty `TMUX`/`STY`/`ZELLIJ` counts;
        /// an empty string (set but blank) does not, like an unset var.
        #[test]
        fn multiplexer_env_detection() {
            // None set.
            assert!(!multiplexer_detected(None, None, None));
            // Empty strings don't count (parity with `env::var_os` + non-empty).
            assert!(!multiplexer_detected(Some(""), Some(""), Some("")));

            // Each var alone triggers.
            assert!(multiplexer_detected(Some("/tmp/tmux-1000"), None, None));
            assert!(multiplexer_detected(None, Some("12345.tmux"), None));
            assert!(multiplexer_detected(None, None, Some("0")));

            // One set among others.
            assert!(multiplexer_detected(None, Some(""), Some("1")));
        }

        /// Runtime switching mutates the live choice and is reflected by
        /// `current_choice` (no terminal probe is involved for explicit picks).
        #[test]
        fn runtime_switch_updates_choice() {
            let before = current_choice();
            set_image_protocol(ImageProtocol::Halfblocks);
            assert_eq!(current_choice(), ImageProtocol::Halfblocks);
            // Restore so other tests sharing the process-global choice aren't
            // affected.
            set_image_protocol(before);
        }
    }
}
