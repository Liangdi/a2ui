# a2ui-dioxus

[![crates.io](https://img.shields.io/crates/v/a2ui-dioxus.svg)](https://crates.io/crates/a2ui-dioxus)
[![docs.rs](https://docs.rs/a2ui-dioxus/badge.svg)](https://docs.rs/a2ui-dioxus)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

中文 | [English](README.md)

> 📦 Member of the **a2ui** crate ecosystem · Dioxus reactive WebView desktop backend (opt-in)
>
> This crate is the sixth render backend of the [`a2ui`](https://crates.io/crates/a2ui) workspace. For the full overview, see the [root README](https://github.com/Liangdi/a2ui#readme).

Renders an [A2UI](https://github.com/a2ui-project/a2ui) component tree into a **native desktop WebView window**, built on [Dioxus](https://github.com/DioxusLabs/dioxus) (reactive-signals architecture, pinned 0.7). Of the six backends this is the most **architecturally distinct**:

- **Reactive signals** — Dioxus is React-like: runtime state lives in a `Signal` at the root and the UI is a pure read of it. So — unlike the Iced backend's `Message` enum (Elm view/update) or the egui backend's `EditBuffers` bridge (immediate mode borrows the data model for the whole frame) — there is **no message enum, no state bridge**. The signal *is* the interaction channel: any write automatically re-renders the components that subscribed to it.
- **Recursive components** — the whole tree is **one** `A2uiNode` component that renders itself per node (Dioxus supports recursive components natively, unlike Slint's bounded-depth codegen).
- **WebView rendering** — Dioxus desktop renders to a system WebView (WebKitGTK on Linux), so the dark theme is a **CSS stylesheet** (`theme::STYLESHEET`) rather than a set of per-widget style functions, and A2UI component kinds map to ordinary HTML elements + classes.

Button clicks reuse the shared `core::components::dispatch_event` + `apply_event_result` (handed up to the gallery root through an `Rc<dyn Fn(String)>` callback injected via context).

> **Opt-in dependency**: this crate is a **non-default** workspace member (it pulls the wry WebView + tao windowing stack); a plain `cargo build` does not compile it.

## Place in the ecosystem

```
┌────────────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced,dioxus}-gallery   │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced] [+ dioxus]) │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-{slint,egui,bevy,iced,dioxus}     │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor)   │
└────────────────────────────────────────────────────────────────────────────┘
```

`a2ui-dioxus` depends only on [`a2ui-base`](https://crates.io/crates/a2ui-base); it is depended on by `a2ui-dioxus-gallery` and (under the `dioxus` feature) by the umbrella `a2ui`.

## Building

All code is behind the `backend` cargo feature, which pulls in the Dioxus desktop runtime (wry WebView + tao windowing). Without it this crate is an empty shell (no deps beyond `a2ui-base`), keeping the workspace's default build light.

```bash
cargo build -p a2ui-dioxus --features backend
```

On Linux it links **WebKitGTK (`webkit2gtk-4.1`) + GTK 3** (must be installed system-wide — the same GTK/X11 dependency story as the other native-window backends).

## Modules

- [`node`](src/node.rs) — the recursive `A2uiNode` component, matching each A2UI component kind to HTML (the counterpart of iced's `walker` + `components`).
- [`app`](src/app.rs) — the `Gallery` root component (prop-less, reads state from context): sidebar + preview pane + Modal overlay + the Button activation flow (the counterpart of `IcedApp`).
- [`theme`](src/theme.rs) — the full dark Catppuccin-Mocha + green-accent CSS (the counterpart of iced's `style`).

## License

MIT
