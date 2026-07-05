//! Image source resolution + format decoding for the GPUI backend.
//!
//! Mirrors the iced/egui image helpers: every `Image` component source
//! (`http(s)` / `data:` / `file://` / local path) is resolved to bytes via the
//! shared `a2ui-image` crate, then decoded into a gpui [`Image`] keyed by an
//! [`ImageFormat`]. GPUI has no built-in URL loader (its `img()` widget takes a
//! path, an asset name, or an in-memory `Arc<Image>`), so remote `Image` URLs
//! must be fetched out-of-band by [`crate::GpuiApp`] and cached as decoded
//! `Arc<Image>`s — exactly the pattern the iced/egui backends use.

use gpui::{Image, ImageFormat};

/// Resolve a URL to bytes via the shared `a2ui-image` resolver (handles
/// `http(s)` / `data:` / `file://` / local paths). Blocking — run off the UI
/// thread (here, on gpui's background executor inside the `cx.spawn` fetch
/// task in [`crate::GpuiApp`]).
pub(crate) fn resolve_bytes(url: &str) -> Option<Vec<u8>> {
    a2ui_image::resolve_bytes(url)
}

/// Decode resolved bytes into a gpui [`Image`], inferring the [`ImageFormat`]
/// from the URL extension / `data:` mime / magic bytes. Returns `None` for an
/// undecodable or unsupported payload so the caller records a failed attempt
/// and keeps the placeholder.
pub(crate) fn decode_image(url: &str, bytes: &[u8]) -> Option<Image> {
    let format = detect_format(url, bytes)?;
    Some(Image::from_bytes(format, bytes.to_vec()))
}

/// Sniff the [`ImageFormat`] from (1) a `data:` URI's mime, (2) the URL's file
/// extension, (3) the payload's magic bytes. Returns `None` for an unknown
/// payload so the caller falls back to the placeholder.
fn detect_format(url: &str, bytes: &[u8]) -> Option<ImageFormat> {
    if let Some(fmt) = from_data_uri_mime(url) {
        return Some(fmt);
    }
    if let Some(fmt) = from_extension(url) {
        return Some(fmt);
    }
    from_magic(bytes)
}

/// Parse a `data:image/<subtype>[;...],<payload>` URI's mime subtype. The mime
/// ends at the first `;` (parameter, e.g. `;base64`) or `,` (the payload) —
/// whichever comes first — so a payload containing extra `+`/`<` characters
/// (e.g. an inline `svg+xml,<svg/>`) doesn't leak into the subtype.
fn from_data_uri_mime(url: &str) -> Option<ImageFormat> {
    let rest = url.strip_prefix("data:")?;
    let mime = rest.split([';', ',']).next()?;
    let subtype = mime.strip_prefix("image/")?;
    mime_to_format(subtype)
}

/// Map a URL's final extension (ignoring any `?query` / `#fragment`) to a format.
fn from_extension(url: &str) -> Option<ImageFormat> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        "gif" => Some(ImageFormat::Gif),
        "svg" => Some(ImageFormat::Svg),
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}

/// Map a mime subtype (e.g. `svg+xml`) to a format.
fn mime_to_format(subtype: &str) -> Option<ImageFormat> {
    match subtype {
        "png" => Some(ImageFormat::Png),
        "jpeg" | "jpg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        "gif" => Some(ImageFormat::Gif),
        "svg+xml" => Some(ImageFormat::Svg),
        "bmp" => Some(ImageFormat::Bmp),
        "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}

/// Identify a format from the leading magic bytes. SVG payloads are XML text
/// (start with `<?xml` or `<svg`); the rest are unambiguous binary signatures.
fn from_magic(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 11 && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else if bytes.starts_with(b"BM") {
        Some(ImageFormat::Bmp)
    } else if bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg") {
        Some(ImageFormat::Svg)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_detection() {
        assert_eq!(from_extension("https://x.com/a.png"), Some(ImageFormat::Png));
        assert_eq!(from_extension("https://x.com/a.JPG"), Some(ImageFormat::Jpeg));
        assert_eq!(
            from_extension("https://x.com/a.jpeg?w=40"),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(from_extension("file:///x/a.svg#frag"), Some(ImageFormat::Svg));
        assert_eq!(from_extension("no-ext"), None);
    }

    #[test]
    fn data_uri_mime_detection() {
        assert_eq!(
            from_data_uri_mime("data:image/png;base64,iVBOR="),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            from_data_uri_mime("data:image/svg+xml,<svg/>"),
            Some(ImageFormat::Svg)
        );
        assert_eq!(from_data_uri_mime("https://x.com/a.png"), None);
    }

    #[test]
    fn magic_byte_detection() {
        assert_eq!(from_magic(&[0x89, 0x50, 0x4E, 0x47, 0x0D]), Some(ImageFormat::Png));
        assert_eq!(from_magic(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(ImageFormat::Jpeg));
        assert_eq!(from_magic(b"<?xml version=\"1.0\"?>"), Some(ImageFormat::Svg));
        assert_eq!(from_magic(b"<svg xmlns="), Some(ImageFormat::Svg));
        assert_eq!(from_magic(&[0x00, 0x01, 0x02]), None);
    }
}
