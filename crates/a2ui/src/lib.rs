//! Umbrella crate that re-exports the A2UI backends under the historical
//! `a2ui::core` / `a2ui::tui` paths, so existing `use a2ui::core::...` and
//! `use a2ui::tui::...` imports keep working after the workspace split.
//!
//! The Slint backend is available as `a2ui::slint` under the `slint` cargo
//! feature, the egui backend as `a2ui::egui` under the `egui` cargo feature,
//! the Bevy backend as `a2ui::bevy` under the `bevy` cargo feature, the Iced
//! backend as `a2ui::iced` under the `iced` cargo feature, and the Dioxus
//! backend as `a2ui::dioxus` under the `dioxus` cargo feature — all opt-in
//! because they pull their (heavy) GUI runtimes.

pub use a2ui_base as core;
pub use a2ui_tui as tui;

#[cfg(feature = "slint")]
pub use a2ui_slint as slint;

#[cfg(feature = "egui")]
pub use a2ui_egui as egui;

#[cfg(feature = "bevy")]
pub use a2ui_bevy as bevy;

#[cfg(feature = "iced")]
pub use a2ui_iced as iced;

#[cfg(feature = "dioxus")]
pub use a2ui_dioxus as dioxus;
