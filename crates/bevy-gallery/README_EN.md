# a2ui-bevy-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-bevy-gallery.svg)](https://crates.io/crates/a2ui-bevy-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · Bevy desktop gallery / sample browser
>
> This crate is the demo app (Bevy backend) of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

The Bevy desktop counterpart of [`a2ui-gallery`](https://crates.io/crates/a2ui-gallery) (terminal), [`a2ui-slint-gallery`](https://crates.io/crates/a2ui-slint-gallery) (Slint), and [`a2ui-egui-gallery`](https://crates.io/crates/a2ui-egui-gallery) (egui): it reuses the same embedded A2UI spec samples and the same catalog / function builders, but renders them into a real OS window via [`a2ui-bevy`](https://crates.io/crates/a2ui-bevy) (left sample list + right preview). Because Bevy's widgets are retained entities, interactions stay smooth across frames.

## Where it fits

```
┌───────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   a2ui-bevy-gallery   a2ui-iced-gallery│
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced])                                │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui   a2ui-bevy   a2ui-iced                              │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor)                                      │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Run

```bash
cargo run -p a2ui-bevy-gallery             # the first sample
cargo run -p a2ui-bevy-gallery -- 3        # by 1-based index
cargo run -p a2ui-bevy-gallery -- stepper  # by name substring (case-insensitive)
```

The full numbered sample list is printed to stdout at startup. The renderer uses wgpu; it needs a GPU/wgpu stack.

## Install

```bash
cargo install a2ui-bevy-gallery
```

> During development, set the `A2UI_SPEC_DIR` environment variable to read samples from an on-disk directory instead (handy for testing spec changes without recompiling).

## License

MIT
