# a2ui-egui

[![crates.io](https://img.shields.io/crates/v/a2ui-egui.svg)](https://crates.io/crates/a2ui-egui)
[![docs.rs](https://docs.rs/a2ui-egui/badge.svg)](https://docs.rs/a2ui-egui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · egui immediate-mode desktop backend (optional)
>
> This crate is the third rendering backend of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

Renders an [A2UI](https://github.com/a2ui-project/a2ui) component tree onto a **native desktop window**, built on [egui](https://github.com/emilk/egui) (immediate-mode GUI, pinned 0.34). Unlike the [Slint](https://crates.io/crates/a2ui-slint) backend, egui **supports recursion natively**, so there is no tree flattening or `build.rs` bounded-depth codegen — `walker::render_node` recurses straight into a `&mut egui::Ui`. egui also offers **real interactive native widgets** (TextField / Slider / CheckBox / ComboBox). 16 of the A2UI components render natively (only Video / AudioPlayer are placeholders): every interactive widget writes genuine input back to the data model; DateTimeInput is an editable ISO text field bound to `value`; Tabs has a clickable tab bar; Icons render as emoji via an embedded ~12 KB NotoEmoji subset font; Images render for real via the `image` crate decoding to an `egui::ColorImage` → `TextureHandle` (local paths decode immediately, remote URLs are fetched synchronously before the walk + cached). Button clicks reuse the shared `core::components::dispatch_event` + `apply_event_result`, identical to the other backends; Modals use a native `egui::Window` overlay.

> **Optional dependency**: this crate is a **non-default workspace member** (it pulls in winit + glow); a plain `cargo build` does not compile it.

## Where it fits

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery  │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui            │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor) │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-egui` depends on [`a2ui-base`](https://crates.io/crates/a2ui-base); it is consumed by `a2ui-egui-gallery` and (under the `egui` feature) the umbrella `a2ui`.

## Building

Everything lives behind the `backend` cargo feature, which is what pulls in the egui + eframe runtime. Without that feature the crate is an empty shell (no dependencies beyond `a2ui-base`), keeping the workspace's default build light.

```bash
cargo build -p a2ui-egui --features backend
```

The renderer uses glow (OpenGL); it needs a GL stack but no dedicated GPU driver.

## The immediate-mode state bridge (implementation note)

An egui widget needs a stable `&mut` buffer every frame (to preserve cursor / scroll position and detect value changes), but A2UI values live in the **data model** (re-parsed from `DataContext` each frame). `EditBuffers` — a persistent map indexed by component id — bridges the two: each frame it is **seeded from the data model** (if stale) → the `&mut` is handed to the egui widget → **changes are detected** → collected as a `PendingInteraction` and written back after the whole tree has been walked (once the data model borrow is dropped).

This is isomorphic to the TUI gallery's "drop the borrow, then mutate" and the Slint host's "callback, then redraw".

## Modules

| Module | Responsibility |
|------|------|
| `walker` | Recursively renders the A2UI component tree → `&mut egui::Ui` |
| `app` | `EguiApp` — owns the surface state, drives the immediate-mode render loop |
| `components` | egui implementations of each A2UI component (real native widgets) |
| `images` | Byte fetching + decoding for the `Image` component → `egui::ColorImage` |
| `edit_state` | `EditBuffers` — the immediate-mode ↔ data model state bridge |
| `interaction` | Maps egui interactions back onto the shared core interaction layer |

## License

MIT
