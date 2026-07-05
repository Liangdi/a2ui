//! Bespoke visual theme for the GPUI gallery — a cohesive dark palette with a
//! green accent, mirroring the other backends' Catppuccin-Mocha neutrals so
//! every renderer agrees on the same chrome.
//!
//! gpui-component ships its own theme system ([`ActiveTheme`] / [`Theme`]), so
//! the native widgets (Button primary, Slider, Checkbox, Scrollbar, …) inherit
//! their colors from the global [`Theme`]. [`customize`] is called once at boot
//! to flip it to dark mode and retint its `primary` / `accent` to the gallery's
//! green so the native widgets and the bespoke chrome match. The palette
//! constants below layer deliberate chrome (the sidebar, surface, cards, the
//! modal dialog, breadcrumb bar, list rows, inputs) on top where the gallery
//! wants direct control, exactly like the iced/egui `style` modules.
//!
//! [`ActiveTheme`]: gpui_component::ActiveTheme
//! [`Theme`]: gpui_component::Theme

use gpui::{Hsla, Rgba};
#[cfg(feature = "backend")]
use gpui_component::{Theme, theme::ThemeMode};

// ===========================================================================
// Palette — Catppuccin Mocha (exact hex), grouped darkest → lightest.
// `Rgba` fields are all `pub f32`, so a `const fn` constructor works (gpui's
// own `rgb()`/`hsla()` are non-const runtime fns).
// ===========================================================================

/// Compose an opaque [`Rgba`] from exact 8-bit RGB.
const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

/// Compose a translucent [`Rgba`] from 8-bit RGB + a 0.0–1.0 alpha.
const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Rgba {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}

/// Darkest base — the whole-app background behind every panel.
pub(crate) const CRUST: Rgba = rgb(0x11, 0x11, 0x1B);
/// Sidebar / top-bar tint (one step lighter than the app background).
pub(crate) const MANTLE: Rgba = rgb(0x18, 0x18, 0x25);
/// The main preview surface.
pub(crate) const BASE: Rgba = rgb(0x1E, 0x1E, 0x2E);
/// Elevated surface — cards, inputs, selected list rows.
pub(crate) const SURFACE0: Rgba = rgb(0x31, 0x32, 0x44);
/// Hover surface — one step above [`SURFACE0`]. (Reserved for future hover
/// states; kept for palette completeness.)
#[allow(dead_code)]
pub(crate) const SURFACE1: Rgba = rgb(0x45, 0x47, 0x5A);

/// Subtle 1px hairline between panels.
pub(crate) const LINE: Rgba = rgba(0xC6, 0xD0, 0xF5, 0.06);
/// Faint border around cards / inputs.
pub(crate) const EDGE: Rgba = rgba(0xC6, 0xD0, 0xF5, 0.08);

/// Primary text (brightest).
pub(crate) const TEXT: Rgba = rgb(0xCD, 0xD6, 0xF4);
/// Secondary text — labels, list rows at rest.
pub(crate) const SUBTEXT0: Rgba = rgb(0xA6, 0xAD, 0xC8);
/// Tertiary text — hints, captions.
pub(crate) const SUBTEXT1: Rgba = rgb(0x93, 0x99, 0xB0);

/// Accent (green) — the gallery's brand color; primary buttons + selection.
pub(crate) const ACCENT: Rgba = rgb(0x3D, 0xD6, 0x8C);
/// Brighter accent for hover states. (Reserved for future hover styling; kept
/// for palette completeness.)
#[allow(dead_code)]
pub(crate) const ACCENT_HI: Rgba = rgb(0x6B, 0xE9, 0xB0);
/// Translucent accent — selected-row washes, focus rings.
pub(crate) const ACCENT_WASH: Rgba = rgba(0x3D, 0xD6, 0x8C, 0.16);

/// Dimmed scrim painted behind an open Modal overlay.
pub(crate) const SCRIM: Rgba = rgba(0x00, 0x00, 0x00, 0.55);

/// The gallery's accent color as GPUI's native [`Hsla`] (for the few APIs that
/// require `Hsla` rather than `impl Into<Fill>`).
pub(crate) const fn accent_hsla() -> Hsla {
    gpui_to_hsla(ACCENT)
}

/// Convert an [`Rgba`] constant into a [`Hsla`] at compile time (gpui's runtime
/// `Rgba::into()` is not `const`). Uses the same conversion gpui applies at
/// runtime; values taken from the standard sRGB→HSL formula.
const fn gpui_to_hsla(c: Rgba) -> Hsla {
    let r = c.r;
    let g = c.g;
    let b = c.b;
    let max = if r >= g && r >= b { r } else if g >= b { g } else { b };
    let min = if r <= g && r <= b { r } else if g <= b { g } else { b };
    let l = (max + min) * 0.5;
    let d = max - min;
    // Saturation.
    let s = if d == 0.0 {
        0.0
    } else if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    // Hue (in degrees, then normalized to 0..1).
    let h_deg = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let h = if h_deg < 0.0 { (h_deg + 360.0) / 360.0 } else { h_deg / 360.0 };
    Hsla { h, s, l, a: c.a }
}

/// Customize the global gpui-component [`Theme`] once at boot: dark mode + the
/// gallery's green primary/accent, so every native widget that isn't restyled
/// explicitly (Slider, Checkbox, Scrollbar, focus rings, …) still agrees with
/// the bespoke chrome. Mirrors the iced `theme()` palette roots. Call this
/// after `gpui_component::init` in the host's launch closure.
#[cfg(feature = "backend")]
pub fn customize(cx: &mut gpui::App) {
    // Flip to dark mode (window arg `None` — applied globally).
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.primary = accent_hsla();
    theme.accent = accent_hsla();
}
