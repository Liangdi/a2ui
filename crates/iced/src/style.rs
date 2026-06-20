//! Bespoke visual theme for the Iced gallery — a cohesive dark palette with a
//! green accent, plus a set of style functions used by [`crate::IcedApp`] and
//! [`crate::components`] to give the gallery a modern, polished look (rounded
//! cards, an indented sidebar, a dimmed modal scrim, etc.).
//!
//! The base [`Theme`] is a custom palette built on the same dark neutrals as
//! Catppuccin Mocha but with a **green primary**, so every native widget the
//! backend does not explicitly restyle (slider, checkbox, scrollbar, rule, …)
//! inherits the green accent too — the bespoke chrome and the native widgets
//! agree. The constants and style fns here layer bespoke chrome on top where
//! the gallery wants deliberate control: the sidebar, the surface background,
//! cards, the modal dialog, the breadcrumb bar, list rows, buttons and inputs.
//!
//! Each style fn has the signature a widget's `.style(…)` expects —
//! `fn(&Theme) -> container::Style`, `fn(&Theme, button::Status) ->
//! button::Style`, … — so they can be passed directly as function items
//! (`button::Style` fns are status-aware to render hover / pressed states).

use iced::widget::{button, container, pick_list as pick_list_w, rule, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

// ===========================================================================
// Palette — Catppuccin Mocha (exact hex), grouped darkest → lightest.
// ===========================================================================

/// Compose an opaque [`Color`] from exact 8-bit RGB.
const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// Compose a translucent [`Color`] from 8-bit RGB + a 0.0–1.0 alpha.
const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

/// Darkest base — the whole-app background behind every panel.
pub(crate) const CRUST: Color = rgb(0x11, 0x11, 0x1B);
/// Sidebar / top-bar tint (one step lighter than the app background).
pub(crate) const MANTLE: Color = rgb(0x18, 0x18, 0x25);
/// The main preview surface.
pub(crate) const BASE: Color = rgb(0x1E, 0x1E, 0x2E);
/// Elevated surface — cards, inputs, selected list rows.
pub(crate) const SURFACE0: Color = rgb(0x31, 0x32, 0x44);
/// Hover surface — one step above [`SURFACE0`].
pub(crate) const SURFACE1: Color = rgb(0x45, 0x47, 0x5A);

/// Subtle 1px hairline between panels.
pub(crate) const LINE: Color = rgba(0xC6, 0xD0, 0xF5, 0.06);
/// Faint border around cards / inputs.
pub(crate) const EDGE: Color = rgba(0xC6, 0xD0, 0xF5, 0.08);

/// Primary text (brightest).
pub(crate) const TEXT: Color = rgb(0xCD, 0xD6, 0xF4);
/// Secondary text — labels, list rows at rest.
pub(crate) const SUBTEXT0: Color = rgb(0xA6, 0xAD, 0xC8);
/// Tertiary text — hints, captions.
pub(crate) const SUBTEXT1: Color = rgb(0x93, 0x99, 0xB0);

/// Accent (green) — the gallery's brand color; primary buttons + selection.
pub(crate) const ACCENT: Color = rgb(0x3D, 0xD6, 0x8C);
/// Brighter accent for hover states.
pub(crate) const ACCENT_HI: Color = rgb(0x6B, 0xE9, 0xB0);
/// Translucent accent — selected-row washes, focus rings.
pub(crate) const ACCENT_WASH: Color = rgba(0x3D, 0xD6, 0x8C, 0.16);

/// Semantic hues (kept as constants so widget fns can tint by meaning). Some
/// are not yet referenced — they form the palette future widget states (success
/// / danger / warning buttons, validation) will draw from.
#[allow(dead_code)]
pub(crate) const GREEN: Color = rgb(0xA6, 0xE3, 0xA1);
#[allow(dead_code)]
pub(crate) const RED: Color = rgb(0xF3, 0x8B, 0xA8);
#[allow(dead_code)]
pub(crate) const PEACH: Color = rgb(0xFA, 0xB3, 0x87);

/// The gallery's base [`Theme`] — a custom dark palette with a green primary,
/// built on the same dark neutrals as Catppuccin Mocha. Using a custom
/// [`Palette`] (rather than `Theme::CatppuccinMocha`) makes every native widget
/// the backend does not restyle (slider, checkbox, scrollbar, rule, …) inherit
/// the green primary too, so the bespoke chrome and the native widgets agree.
pub fn theme() -> Theme {
    Theme::custom(
        "A2UI Green",
        iced::theme::Palette {
            background: BASE,
            text: TEXT,
            primary: ACCENT,
            success: GREEN,
            warning: PEACH,
            danger: RED,
        },
    )
}

// ===========================================================================
// Container styles — backgrounds, cards, bars, the modal panel + scrim.
// ===========================================================================

/// Whole-app backdrop (crust) painted behind the sidebar + preview row.
pub(crate) fn app_bg(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(CRUST)),
        ..container::Style::default()
    }
}

/// Sidebar panel — mantle fill, a hairline on its right edge.
pub(crate) fn sidebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(MANTLE)),
        border: Border {
            color: LINE,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

/// Top breadcrumb bar over the preview — mantle fill, no border (a rule sits
/// beneath it to draw the separator).
pub(crate) fn top_bar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(MANTLE)),
        ..container::Style::default()
    }
}

/// The preview scroll-area surface — base fill.
pub(crate) fn surface(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BASE)),
        ..container::Style::default()
    }
}

/// A rounded, softly-elevated card (the A2UI `Card` component, modal sections).
pub(crate) fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE0)),
        border: Border {
            color: EDGE,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: rgba(0x00, 0x00, 0x00, 0.30),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..container::Style::default()
    }
}

/// A small rounded "chip" used for placeholder widgets (Image / Video / Icon /
/// ChoicePicker) so they read as intentional badges rather than bracket text.
pub(crate) fn chip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE0)),
        border: Border {
            color: EDGE,
            width: 1.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

/// A pill-shaped index counter shown beside the current sample in the
/// breadcrumb bar.
pub(crate) fn index_pill(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ACCENT_WASH)),
        border: Border {
            color: rgba(0x3D, 0xD6, 0x8C, 0.25),
            width: 1.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

/// The centered modal dialog panel — elevated, large soft shadow, rounded.
pub(crate) fn modal_panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BASE)),
        border: Border {
            color: EDGE,
            width: 1.0,
            radius: 16.0.into(),
        },
        shadow: Shadow {
            color: rgba(0x00, 0x00, 0x00, 0.55),
            offset: Vector::new(0.0, 16.0),
            blur_radius: 48.0,
        },
        ..container::Style::default()
    }
}

// ===========================================================================
// Rule styles — subtle separators that match the dark palette.
// ===========================================================================

/// A faint horizontal divider (used between sections / under the top bar).
pub(crate) fn divider(_: &Theme) -> rule::Style {
    rule::Style {
        color: LINE,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

// ===========================================================================
// Button styles — list rows, primary/secondary actions, the modal scrim.
// ===========================================================================

/// A selectable sidebar list row. `selected` swaps the resting fill + text to
/// the accent wash + bright text and adds a rounded highlight; hover lifts the
/// background a shade so unselected rows still feel interactive.
pub(crate) fn sample_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, text) = if selected {
            (Some(Background::Color(ACCENT_WASH)), TEXT)
        } else if hovered {
            (Some(Background::Color(SURFACE0)), SUBTEXT0)
        } else {
            (None, SUBTEXT0)
        };
        button::Style {
            background: bg,
            text_color: text,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// A primary action button — solid accent fill, dark text, soft shadow,
/// brightening on hover.
pub(crate) fn primary(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => ACCENT_HI,
        button::Status::Pressed => ACCENT,
        button::Status::Disabled => SURFACE1,
        button::Status::Active => ACCENT,
    };
    let text = if matches!(status, button::Status::Disabled) {
        SUBTEXT1
    } else {
        CRUST
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 9.0.into(),
        },
        shadow: Shadow {
            color: rgba(0x3D, 0xD6, 0x8C, 0.30),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        snap: true,
    }
}

/// A secondary / default button — surface fill, faint edge, hover lift.
pub(crate) fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let (bg, edge) = match status {
        button::Status::Hovered => (SURFACE1, EDGE),
        button::Status::Pressed => (SURFACE0, EDGE),
        button::Status::Disabled => (SURFACE0, EDGE),
        button::Status::Active => (SURFACE0, EDGE),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: TEXT,
        border: Border {
            color: edge,
            width: 1.0,
            radius: 9.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

/// A borderless text button — accent text, faint wash on hover (used for the
/// modal "✕" close affordance and `variant: borderless` A2UI buttons).
pub(crate) fn borderless(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then(|| Background::Color(SURFACE0)),
        text_color: if hovered { ACCENT_HI } else { ACCENT },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 7.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

/// The modal scrim — a full-viewport, semi-opaque click target behind the
/// dialog. Rendered as a button so a click dismisses the modal.
pub(crate) fn scrim(_: &Theme, _: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(rgba(0x04, 0x05, 0x09, 0.66))),
        text_color: Color::TRANSPARENT,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

/// A Tabs title button. `active` swaps the resting fill to the accent wash +
/// bright text so the selected tab reads clearly above the rule beneath the bar;
/// inactive tabs are transparent with muted text, lifting to a surface wash on
/// hover so they still feel clickable.
pub(crate) fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, text) = if active {
            (Some(Background::Color(ACCENT_WASH)), TEXT)
        } else if hovered {
            (Some(Background::Color(SURFACE0)), SUBTEXT0)
        } else {
            (None, SUBTEXT1)
        };
        button::Style {
            background: bg,
            text_color: text,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 7.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

// ===========================================================================
// Input styles — a rounded, recessed text field that brightens on focus.
// ===========================================================================

/// A recessed text input: mantle fill, faint edge, accent ring when focused.
pub(crate) fn text_field(_: &Theme, status: text_input::Status) -> text_input::Style {
    let (bg, border) = match status {
        text_input::Status::Focused { .. } => (
            SURFACE0,
            Border {
                color: ACCENT,
                width: 1.5,
                radius: 9.0.into(),
            },
        ),
        text_input::Status::Hovered => (
            SURFACE0,
            Border {
                color: OVERLAY_EDGE,
                width: 1.0,
                radius: 9.0.into(),
            },
        ),
        _ => (
            MANTLE,
            Border {
                color: EDGE,
                width: 1.0,
                radius: 9.0.into(),
            },
        ),
    };
    text_input::Style {
        background: Background::Color(bg),
        border,
        icon: SUBTEXT0,
        placeholder: SUBTEXT1,
        value: TEXT,
        selection: ACCENT_WASH,
    }
}

/// A slightly stronger edge for hovered inputs (kept separate from [`EDGE`] so
/// the hover state reads as a deliberate lift).
const OVERLAY_EDGE: Color = rgba(0xC6, 0xD0, 0xF5, 0.14);

/// A ChoicePicker dropdown — recessed like the text field, with an accent
/// handle. Mirrors [`text_field`]'s focus/hover lift so the two input kinds
/// agree.
pub(crate) fn pick_list(_: &Theme, status: pick_list_w::Status) -> pick_list_w::Style {
    let (bg, border) = match status {
        pick_list_w::Status::Opened { .. } => (
            SURFACE0,
            Border {
                color: ACCENT,
                width: 1.5,
                radius: 9.0.into(),
            },
        ),
        pick_list_w::Status::Hovered => (
            SURFACE0,
            Border {
                color: OVERLAY_EDGE,
                width: 1.0,
                radius: 9.0.into(),
            },
        ),
        pick_list_w::Status::Active => (
            MANTLE,
            Border {
                color: EDGE,
                width: 1.0,
                radius: 9.0.into(),
            },
        ),
    };
    pick_list_w::Style {
        text_color: TEXT,
        placeholder_color: SUBTEXT1,
        handle_color: ACCENT,
        background: Background::Color(bg),
        border,
    }
}
