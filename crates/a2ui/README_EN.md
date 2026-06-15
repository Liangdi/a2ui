# a2ui

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 The **a2ui** crate ecosystem · umbrella crate (single entry point)
>
> This is the umbrella crate of the [`a2ui`](https://crates.io/crates/a2ui) workspace, re-exporting the sub-crates under the stable `a2ui::core` / `a2ui::tui` paths. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

A Rust implementation of the [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) protocol: render user interfaces that AI agents generate dynamically and drive over a JSON stream. This crate is the **umbrella** — one dependency gets you the core layer and the default terminal backend; the Slint / egui desktop backends are opt-in behind features.

## Ecosystem overview

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:  a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   │
├───────────────────────────────────────────────────────────────────────┤
│  ▶ a2ui  (umbrella: re-export core + tui [+ slint] [+ egui])          │
├───────────────────────────────────────────────────────────────────────┤
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor) │
└───────────────────────────────────────────────────────────────────────┘
```

| Sub-crate | Role | Path under the umbrella |
|----------|------|----------------------|
| [`a2ui-base`](https://crates.io/crates/a2ui-base) | Framework-agnostic core | `a2ui::core` |
| [`a2ui-tui`](https://crates.io/crates/a2ui-tui) | ratatui terminal backend (default) | `a2ui::tui` |
| [`a2ui-slint`](https://crates.io/crates/a2ui-slint) | Slint desktop backend (optional) | `a2ui::slint` (`slint` feature) |
| [`a2ui-egui`](https://crates.io/crates/a2ui-egui) | egui desktop backend (optional) | `a2ui::egui` (`egui` feature) |

> By default only `core` + `tui` are re-exported. The two desktop backends are heavy (Slint toolchain / winit + glow), so they are opt-in.

## Features

| Feature | Description | Enable |
|------|------|------|
| `slint` | Re-export the Slint backend as `a2ui::slint` | `--features slint` |
| `egui` | Re-export the egui backend as `a2ui::egui` | `--features egui` |
| `audio` | Forwarded to `a2ui-tui` for real audio playback | `--features audio` |

## Usage

```bash
cargo add a2ui                  # core + default terminal backend
cargo add a2ui --features egui  # also enable the egui desktop backend
```

```rust
// The paths stay stable — that's the whole point of the umbrella
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
```

## Examples

This crate ships 17 examples — the best on-ramp to A2UI:

```bash
cargo run -p a2ui --example 01_hello_world
cargo run -p a2ui --example 04_login_form
cargo run -p a2ui --example 12_handshake      # capabilities handshake
```

See the [root README](https://github.com/Liangdi/a2ui#readme) for the full example table.

## License

MIT
