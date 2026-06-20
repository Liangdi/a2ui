//! Image decoding for the `Image` component — decode raster bytes into an
//! `egui::ColorImage`, cached as a `TextureHandle` on [`crate::EguiApp`] (the
//! handle itself is created in `EguiApp::load_images`, which holds the
//! `egui::Context`).
//!
//! This is the egui counterpart of the Bevy backend's `images.rs`, the Iced
//! backend's `fetch_sample_images` / `fetch_handle`, and the Slint backend's
//! inline image decode. Like Bevy and Slint, egui's gallery has no convenient
//! async hook, so decoding runs **synchronously on the UI thread** in
//! [`EguiApp::load_images`] (a read-pass that collects uncached URLs, then a
//! write-pass that decodes + caches them). The gallery samples carry only a
//! handful of small images, so the one-time per-URL cost (a few ms for local
//! files; one blocking HTTP round-trip per remote URL with a 5 s cap) is
//! acceptable. Results are cached by resolved URL and cleared on sample switch.
//!
//! - **local file** (`path`, `file://path`): read + decode via the `image` crate.
//! - **`http(s)` URL**: blocking `ureq` GET → decode.
//! - **`data:` URL / decode failure / missing file**: `None` (placeholder stays,
//!   not retried), matching the Slint/Bevy backends.

use std::time::Duration;

use egui::ColorImage;

/// Fetch + decode one image URL to an `egui::ColorImage` (or `None` on any
/// failure / unsupported scheme). Blocking — see the module docs.
pub fn decode_url(url: &str) -> Option<ColorImage> {
    let bytes = fetch_bytes(url)?;
    decode_bytes(&bytes)
}

/// Fetch the raw bytes for `url`: blocking `ureq` GET for `http(s)`, a local
/// file read for a path / `file://` URL, and `None` for `data:` URLs (matching
/// the Slint/Bevy backends).
fn fetch_bytes(url: &str) -> Option<Vec<u8>> {
    if url.starts_with("http://") || url.starts_with("https://") {
        // ureq 3: per-request `.timeout()` is gone — timeouts live on the
        // `Agent` config. Build a short-lived agent per fetch (these are
        // low-volume gallery image downloads, not a hot path).
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into();
        let mut resp = agent.get(url).call().ok()?;
        resp.body_mut().read_to_vec().ok()
    } else if url.starts_with("data:") {
        None
    } else {
        let path = url.strip_prefix("file://").unwrap_or(url);
        std::fs::read(path).ok()
    }
}

/// Decode encoded image bytes (PNG / JPEG / …) into an `egui::ColorImage` via
/// the standalone `image` crate (same 0.25 line Bevy/Iced use). Returns `None`
/// for an undecodable payload (the placeholder stays). The `image` crate yields
/// non-premultiplied straight RGBA, so `from_rgba_unmultiplied` is correct here.
fn decode_bytes(bytes: &[u8]) -> Option<ColorImage> {
    let dyn_img = image::load_from_memory(bytes).ok()?;
    let rgba = dyn_img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &rgba.into_raw(),
    ))
}
