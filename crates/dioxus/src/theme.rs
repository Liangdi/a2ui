//! Bespoke visual theme for the Dioxus gallery — the CSS counterpart of the
//! Iced backend's [`style`](../../iced/src/style.rs) module.
//!
//! Dioxus desktop renders to a system WebView, so the gallery's look is a real
//! stylesheet injected into the document head (see
//! [`crate::theme::STYLESHEET`]) rather than a set of per-widget style
//! functions. The palette is the exact same dark Catppuccin-Mocha neutrals with
//! a **green primary** (`#3DD68C`) that the Iced/egui galleries use, so the six
//! backends agree visually: same crust backdrop, the same indented sidebar, the
//! same rounded cards, the same dimmed modal scrim, and the same recessed
//! inputs that ring green on focus.
//!
//! The class names used here are emitted by the per-component render arms in
//! [`crate::node`] (e.g. `.card`, `.btn.btn--primary`, `.field`, `.chip`), so
//! each A2UI component kind maps to a stable, themed class.

/// The whole gallery stylesheet, injected via the desktop `Config`'s custom
/// `<head>`. A single constant keeps the theme in one place (no per-widget
/// style fns needed) — the WebView's CSS engine does the rest.
///
/// Palette (Catppuccin Mocha + green accent), darkest → lightest:
///
/// | token      | hex       | use                       |
/// |------------|-----------|---------------------------|
/// | `--crust`  | `#11111B` | whole-app backdrop        |
/// | `--mantle` | `#181825` | sidebar / top bar         |
/// | `--base`   | `#1E1E2E` | preview surface           |
/// | `--s0`     | `#313244` | cards / inputs / selected |
/// | `--s1`     | `#45475A` | hover                     |
/// | `--text`   | `#CDD6F4` | primary text              |
/// | `--sub0`   | `#A6ADC8` | secondary text            |
/// | `--sub1`   | `#9399B0` | tertiary text             |
/// | `--acc`    | `#3DD68C` | accent (green)            |
pub const STYLESHEET: &str = r#"
:root {
  --crust: #11111B; --mantle: #181825; --base: #1E1E2E;
  --s0: #313244; --s1: #45475A;
  --text: #CDD6F4; --sub0: #A6ADC8; --sub1: #9399B0;
  --acc: #3DD68C; --acc-hi: #6BE9B0;
  --line: rgba(198,208,245,0.06); --edge: rgba(198,208,245,0.08);
  --acc-wash: rgba(61,214,140,0.16);
}
* { box-sizing: border-box; }
html, body { height: 100%; margin: 0; }
body {
  background: var(--crust); color: var(--text);
  font: 13px/1.5 -apple-system, "Segoe UI", system-ui, sans-serif;
  -webkit-font-smoothing: antialiased;
}
.mono { font-family: "JetBrains Mono", "SF Mono", "Cascadia Code", ui-monospace, monospace; }

/* ── Layout shell ─────────────────────────────────────────────── */
.app { display: flex; width: 100%; height: 100vh; }
.sidebar {
  width: 248px; flex-shrink: 0; height: 100%;
  background: var(--mantle); border-right: 1px solid var(--line);
  padding: 16px; display: flex; flex-direction: column; overflow: hidden;
}
.sidebar__brand { display: flex; align-items: center; gap: 10px; padding: 2px 0; }
.sidebar__mark { color: var(--acc); font-size: 18px; }
.sidebar__title { display: flex; flex-direction: column; }
.sidebar__title b { font-size: 15px; color: var(--text); font-weight: 600; }
.sidebar__title span { font-size: 11px; color: var(--sub1); }
.sidebar__section { font-size: 10px; color: var(--sub1); letter-spacing: 0.06em; }
.sidebar__list { display: flex; flex-direction: column; gap: 4px; flex: 1; overflow-y: auto; }
.sidebar__foot { font-size: 10px; color: var(--sub1); }
.main { flex: 1; display: flex; flex-direction: column; min-width: 0; height: 100%; }
.topbar {
  display: flex; align-items: center; gap: 8px;
  background: var(--mantle); padding: 14px 20px;
}
.topbar__crumb, .topbar__sep { font-size: 12px; color: var(--sub1); }
.topbar__title { font-size: 14px; color: var(--text); }
.topbar__chip {
  font-size: 11px; color: var(--acc); padding: 3px 8px;
  background: var(--acc-wash); border: 1px solid rgba(61,214,140,0.25);
  border-radius: 999px;
}
.spacer { flex: 1; }
.preview { flex: 1; background: var(--base); padding: 24px; overflow-y: auto; }
hr { border: 0; border-top: 1px solid var(--line); margin: 12px 0; }

/* ── Sidebar sample rows ──────────────────────────────────────── */
.sample {
  display: flex; align-items: center; gap: 10px; text-align: left;
  padding: 8px; width: 100%; border: 0; border-radius: 8px;
  background: transparent; color: var(--sub0); font: inherit; cursor: pointer;
}
.sample:hover { background: var(--s0); color: var(--sub0); }
.sample--sel { background: var(--acc-wash); color: var(--text); }
.sample__idx { width: 20px; font-size: 11px; color: var(--sub1); }
.sample--sel .sample__idx { color: var(--acc); }
.sample__name { font-size: 13px; }

/* ── Rendered A2UI tree ───────────────────────────────────────── */
.col { display: flex; flex-direction: column; gap: 8px; }
.row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.card {
  background: var(--s0); border: 1px solid var(--edge);
  border-radius: 12px; padding: 18px; width: 100%;
  box-shadow: 0 2px 8px rgba(0,0,0,0.30);
}
.text { color: var(--text); }
.text--h1 { font-size: 28px; font-weight: 700; }
.text--h2 { font-size: 22px; font-weight: 600; }
.text--h3 { font-size: 18px; font-weight: 600; }
.muted { color: var(--sub0); }

/* ── Interactive widgets ──────────────────────────────────────── */
.btn {
  display: inline-flex; align-items: center; justify-content: center;
  padding: 9px 16px; border-radius: 9px; border: 0; cursor: pointer;
  font: inherit; color: var(--text); background: var(--s0); border: 1px solid var(--edge);
  transition: background .12s;
}
.btn:hover { background: var(--s1); }
.btn--primary {
  background: var(--acc); color: var(--crust); font-weight: 600;
  border: 0; box-shadow: 0 2px 8px rgba(61,214,140,0.30);
}
.btn--primary:hover { background: var(--acc-hi); }
.btn--borderless {
  background: transparent; color: var(--acc); border: 0; padding: 4px 8px;
}
.btn--borderless:hover { background: var(--s0); color: var(--acc-hi); }
.btn:disabled { opacity: .45; cursor: not-allowed; }

.field { display: flex; flex-direction: column; gap: 6px; width: 100%; }
.field > .label { font-size: 12px; color: var(--sub0); }
.field > input, .field > textarea {
  font: inherit; color: var(--text); padding: 9px 12px;
  background: var(--mantle); border: 1px solid var(--edge); border-radius: 9px;
  width: 100%; outline: none;
}
.field > input:focus { border-color: var(--acc); box-shadow: 0 0 0 1.5px var(--acc); background: var(--s0); }
.check { display: inline-flex; align-items: center; gap: 8px; cursor: pointer; color: var(--text); }
.check > input { width: 16px; height: 16px; accent-color: var(--acc); }
.range { width: 100%; accent-color: var(--acc); }

/* ── Placeholder chips (Icon / Image / Video / Audio / ChoicePicker) ── */
.chip {
  display: inline-flex; align-items: center; gap: 8px;
  padding: 6px 12px; border-radius: 999px;
  background: var(--s0); border: 1px solid var(--edge); font-size: 12px;
}
.chip__glyph { color: var(--acc); font-size: 13px; }
.chip__label { color: var(--sub0); }
.unknown { color: var(--sub1); }

/* ── Modal overlay ────────────────────────────────────────────── */
.scrim {
  position: fixed; inset: 0; background: rgba(4,5,9,0.66);
  border: 0; padding: 0; cursor: default;
}
.modal-wrap { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center; z-index: 10; }
.modal {
  width: 480px; max-width: 560px; background: var(--base);
  border: 1px solid var(--edge); border-radius: 16px; padding: 24px;
  box-shadow: 0 16px 48px rgba(0,0,0,0.55);
}
.modal__head { display: flex; align-items: center; gap: 8px; margin-bottom: 14px; }
.modal__title { font-size: 14px; color: var(--text); }

/* Slim scrollbars to match the dark palette. */
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb { background: var(--s0); border-radius: 999px; }
::-webkit-scrollbar-track { background: transparent; }
"#;
