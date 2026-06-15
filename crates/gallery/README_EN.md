# a2ui-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-gallery.svg)](https://crates.io/crates/a2ui-gallery)
[![docs.rs](https://docs.rs/a2ui-gallery/badge.svg)](https://docs.rs/a2ui-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · terminal gallery / sample browser
>
> This crate is the demo app (ratatui backend) of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

A **terminal app** (bin + lib) that browses and step-by-step renders the [A2UI](https://github.com/a2ui-project/a2ui) spec samples. It embeds the official A2UI spec tree into the binary at compile time (`include_dir`), so the distributed binary needs no spec directory on disk. The desktop Slint / egui galleries reuse this crate's `sample_loader` to load the same samples.

## Where it fits

```
┌───────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery│
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor) │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-gallery` is also published as a library (`a2ui_gallery`); its `sample_loader` is shared by all three gallery apps, and `a2ui-slint-gallery` / `a2ui-egui-gallery` depend on it directly.

## Install / Run

```bash
# Run directly
cargo run -p a2ui-gallery

# Install as the system binary a2ui_gallery
cargo install a2ui-gallery
```

### Controls

| Key | Action |
|------|------|
| `↑`/`k`, `↓`/`j` | Navigate the sample list |
| `Enter` | Select the current sample and render it |
| `n` | Process the next message step |
| `a` | Process all remaining messages |
| `r` | Reset and replay |
| `Tab` | Cycle focus |
| `Esc` | Back to list / quit |

## As a library

```rust
use a2ui_gallery::sample_loader;

// Load a catalog's samples from the embedded spec tree
let samples = sample_loader::load_samples("v1_0/catalogs/basic/examples");
```

> During development, set the `A2UI_SPEC_DIR` environment variable to read samples from an on-disk directory instead (handy for testing spec changes without recompiling).

## License

MIT
