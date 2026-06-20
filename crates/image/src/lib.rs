//! Shared image source resolution + raster decode for every A2UI backend.
//!
//! Before this crate existed, each GUI backend (egui / slint / bevy / iced)
//! re-implemented its own `fetch_bytes` / `decode_bytes`, and they had drifted:
//! egui and bevy treated `data:` URIs as unsupported, slint handled only `http`,
//! iced only `http`. The same A2UI spec therefore rendered images differently
//! per backend. This module is the single source of truth — backends call
//! [`resolve_bytes`] (and, for raster payloads, [`decode`]) and wrap the result
//! into their own native image type.
//!
//! The [`decode`] return type is a plain `{ width, height, rgba }` struct so no
//! `image`-crate type leaks across the API — a backend does not need to depend
//! on the `image` crate merely to name a return type.

use std::time::Duration;

use base64::Engine;
use image::ImageReader;

/// A decoded raster image: raw 8-bit RGBA pixels plus dimensions. No
/// `image`-crate type is exposed, so callers can stay free of that dependency.
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The parsed body of a `data:` URI (the text after `data:`).
///
/// Exposed so the TUI backend can branch on `mime == "image/svg+xml"` and
/// rasterize via `resvg` itself, while delegating the base64 / percent-decoding
/// (the part shared with every other backend) to [`parse_data_uri`].
pub struct DataUri {
    /// Lowercased MIME type, e.g. `"image/png"` or `"image/svg+xml"`.
    pub mime: String,
    /// Raw (already base64- or percent-decoded) payload bytes.
    pub bytes: Vec<u8>,
}

/// Resolve any image URL to its raw bytes.
///
/// Supported sources:
/// - `http://` / `https://` — fetched synchronously via `ureq` (5 s timeout).
/// - `data:` URI — body parsed via [`parse_data_uri`] (base64 or percent-encoded).
/// - `file://…` or a bare local path — read from disk.
///
/// Returns `None` on any failure (network, decode, IO) so the caller can fall
/// back to its placeholder. This runs on the calling thread; the GUI backends
/// keep these fetches to a handful of small gallery images.
pub fn resolve_bytes(url: &str) -> Option<Vec<u8>> {
    if let Some(rest) = url.strip_prefix("data:") {
        return parse_data_uri(rest).map(|du| du.bytes);
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        // ureq 3: timeouts live on the `Agent` config, not the per-request
        // builder. Build a short-lived agent per fetch — these are low-volume
        // gallery image downloads, not a hot path.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into();
        let mut resp = agent.get(url).call().ok()?;
        return resp.body_mut().read_to_vec().ok();
    }
    let path = url.strip_prefix("file://").unwrap_or(url);
    std::fs::read(path).ok()
}

/// Parse the body of a `data:` URI (the text after `data:`).
///
/// Accepts both the `;base64` form (binary payloads, e.g. PNG/JPEG) and the
/// URL-percent-encoded form (the common inline case). Returns the lowercased
/// MIME type and the decoded bytes. `None` if the URI has no `,` separator.
pub fn parse_data_uri(rest: &str) -> Option<DataUri> {
    let (metadata, data) = rest.split_once(',')?;
    let is_base64 = metadata.contains("base64");
    // MIME type is the first `;`-delimited segment, e.g. "image/svg+xml".
    let mime = metadata
        .split(';')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    let bytes: Vec<u8> = if is_base64 {
        base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .ok()?
    } else {
        // Percent-decode into raw bytes (binary-safe; SVG bytes are UTF-8).
        percent_encoding::percent_decode_str(data).collect()
    };

    Some(DataUri { mime, bytes })
}

/// Decode a raster image payload (PNG / JPEG / …) into RGBA pixels.
///
/// Wraps `image::load_from_memory` → `to_rgba8`. Returns `None` for an
/// undecodable payload so the caller can keep its placeholder. SVG is **not**
/// handled here — it needs `resvg`, which only the TUI backend pulls in.
pub fn decode(bytes: &[u8]) -> Option<DecodedImage> {
    let dyn_img = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let rgba = dyn_img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}
