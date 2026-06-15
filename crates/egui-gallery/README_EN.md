# a2ui-egui-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-egui-gallery.svg)](https://crates.io/crates/a2ui-egui-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · egui desktop gallery / sample browser
>
> This crate is the demo app (egui backend) of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

The egui desktop counterpart of [`a2ui-gallery`](https://crates.io/crates/a2ui-gallery) (terminal) and [`a2ui-slint-gallery`](https://crates.io/crates/a2ui-slint-gallery) (Slint): it reuses the same embedded A2UI spec samples and the same catalog / function builders, but renders them into a real OS window via [`a2ui-egui`](https://crates.io/crates/a2ui-egui) (left sample list + right preview). Thanks to egui's native widgets, the input controls here are **truly interactive**.

## Where it fits

```
┌───────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery│
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor) │
└───────────────────────────────────────────────────────────────────────┘
```

## Run

```bash
cargo run -p a2ui-egui-gallery             # the first sample
cargo run -p a2ui-egui-gallery -- 3        # by 1-based index
cargo run -p a2ui-egui-gallery -- login    # by name substring (case-insensitive)
```

The full numbered sample list is printed to stdout at startup. The renderer uses glow (OpenGL); it needs a GL stack but no dedicated GPU driver.

## Install

```bash
cargo install a2ui-egui-gallery
```

> During development, set the `A2UI_SPEC_DIR` environment variable to read samples from an on-disk directory instead (handy for testing spec changes without recompiling).

## License

MIT
