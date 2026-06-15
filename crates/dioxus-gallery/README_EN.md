# a2ui-dioxus-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-dioxus-gallery.svg)](https://crates.io/crates/a2ui-dioxus-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

中文 | [English](README.md)

> 📦 Member of the **a2ui** crate ecosystem · Dioxus WebView desktop gallery browser
>
> This crate is the showcase app (Dioxus backend) of the [`a2ui`](https://crates.io/crates/a2ui) workspace. For the full overview, see the [root README](https://github.com/Liangdi/a2ui#readme).

The Dioxus desktop counterpart of [`a2ui-gallery`](https://crates.io/crates/a2ui-gallery) (terminal) and the other galleries: it reuses the same embedded A2UI spec samples and the same catalog / function builders, but renders them into a real OS WebView window (left sample list + right preview) via [`a2ui-dioxus`](https://crates.io/crates/a2ui-dioxus). Thanks to the WebView, the input controls are **genuinely interactive** (native HTML form controls); Modals layer as centered panels over a dimmed scrim. The dark theme is a **CSS stylesheet** injected into the document `<head>` (the same Catppuccin-Mocha + green-accent palette as the Iced/egui galleries).

## Place in the ecosystem

```
┌────────────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced,dioxus}-gallery │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced] [+ dioxus]) │
│  backends:   a2ui-tui (ratatui)   a2ui-{slint,egui,bevy,iced,dioxus}       │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor)   │
└────────────────────────────────────────────────────────────────────────────┘
```

## Run

```bash
cargo run -p a2ui-dioxus-gallery             # the first sample
cargo run -p a2ui-dioxus-gallery -- 3        # by 1-based index
cargo run -p a2ui-dioxus-gallery -- login    # by name substring (case-insensitive)
```

The available samples (index + name) are printed to stdout at startup. The window is 1080×740.

> Linux requires **WebKitGTK (`webkit2gtk-4.1`) + GTK 3** installed system-wide.

## How it hands state to the prop-less Gallery

Dioxus's `launch(app)` takes a `fn() -> Element` (a parameter-less function pointer), and component props must be `Clone + PartialEq` (while `MessageProcessor` and the function map are not `Clone`). So the sample list + initial index (both `Clone`) are injected into the root context via `LaunchBuilder::with_context`; the catalogs + function map are rebuilt inside `app()`, packed into `Signal`s / `Rc`, and shared with the prop-less `Gallery` and the recursive `A2uiNode` via context. This differs from the Iced gallery's `Mutex<Option<…>>` boot hack (Iced's boot closure must be `Fn`, whereas Dioxus's `use_signal` initializer is `FnOnce`).

## License

MIT
