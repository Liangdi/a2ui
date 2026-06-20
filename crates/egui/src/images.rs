//! Image decoding for the `Image` component — resolve an image URL and decode
//! raster bytes into an `egui::ColorImage`, cached as a `TextureHandle` on
//! [`crate::EguiApp`] (the handle itself is created in `EguiApp::load_images`,
//! which holds the `egui::Context`).
//!
//! The byte-resolution (`http(s)` / `data:` / `file://`) and raster decode are
//! shared with every other backend via the `a2ui-image` crate — this module only
//! wraps the result into egui's `ColorImage`. Like Bevy and Slint, egui's
//! gallery has no convenient async hook, so decoding runs **synchronously on the
//! UI thread** in [`EguiApp::load_images`] (a read-pass that collects uncached
//! URLs, then a write-pass that decodes + caches them). The gallery samples
//! carry only a handful of small images, so the one-time per-URL cost is
//! acceptable. Results are cached by resolved URL and cleared on sample switch.

use egui::ColorImage;

/// Resolve + decode one image URL into an `egui::ColorImage` (or `None` on any
/// failure). Source resolution and raster decode are delegated to `a2ui-image`;
/// this just maps the resulting RGBA into egui's non-premultiplied `ColorImage`.
/// Blocking — see the module docs.
pub fn decode_url(url: &str) -> Option<ColorImage> {
    let bytes = a2ui_image::resolve_bytes(url)?;
    let img = a2ui_image::decode(&bytes)?;
    Some(ColorImage::from_rgba_unmultiplied(
        [img.width as usize, img.height as usize],
        &img.rgba,
    ))
}
