# a2ui-iced-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-iced-gallery.svg)](https://crates.io/crates/a2ui-iced-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · Iced desktop gallery / sample browser
>
> This crate is the demo app (Iced backend) of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

The Iced desktop counterpart of [`a2ui-gallery`](https://crates.io/crates/a2ui-gallery) (terminal) and the other galleries: it reuses the same embedded A2UI spec samples and the same catalog / function builders, but renders them into a real OS window via [`a2ui-iced`](https://crates.io/crates/a2ui-iced) (left sample list + right preview). Thanks to Iced's native widgets, the input controls here are **truly interactive**, and Modals float as a centered overlay above the main surface.

## Where it fits

```
┌────────────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced}-gallery       │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced]) │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui   a2ui-bevy   a2ui-iced │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor)  │
└────────────────────────────────────────────────────────────────────────────┘
```

## Run

```bash
cargo run -p a2ui-iced-gallery             # the first sample
cargo run -p a2ui-iced-gallery -- 3        # by 1-based index
cargo run -p a2ui-iced-gallery -- login    # by name substring (case-insensitive)
```

The full numbered sample list is printed to stdout at startup. The renderer defaults to wgpu (GPU), with a tiny-skia software fallback.

## Install

```bash
cargo install a2ui-iced-gallery
```

> During development you can set the `A2UI_SPEC_DIR` environment variable to read samples from an on-disk directory instead (handy for testing spec changes without recompiling).

## License

MIT
