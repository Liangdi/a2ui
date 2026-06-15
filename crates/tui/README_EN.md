# a2ui-tui

[![crates.io](https://img.shields.io/crates/v/a2ui-tui.svg)](https://crates.io/crates/a2ui-tui)
[![docs.rs](https://docs.rs/a2ui-tui/badge.svg)](https://docs.rs/a2ui-tui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · default terminal backend (ratatui)
>
> This crate is the default rendering backend of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

Renders an [A2UI](https://github.com/a2ui-project/a2ui) component tree onto a **terminal character grid**, built on [ratatui](https://ratatui.rs/) + [crossterm](https://github.com/crossterm-rs/crossterm). This is a2ui's default backend (a workspace `default-member`) — a plain `cargo build` compiles it.

## Where it fits

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery  │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui            │
├───────────────────────────────────────────────────────────────────────┤
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor) │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-tui` depends on [`a2ui-base`](https://crates.io/crates/a2ui-base); it is consumed by `a2ui-gallery` and the umbrella `a2ui`.

## Features

- ✅ **18 TUI components**: Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
- ✅ Interactive components (arrow-key value adjustment): Text, Row, Column, Button, TextField, Slider, CheckBox, ChoicePicker, DateTimeInput
- ✅ **Real image rendering on by default** (`ratatui-image`; kitty / iTerm2 / Sixel / Halfblocks auto-fallback; local file paths only)
- ✅ Weight-based split / alignment layout engine (`layout_engine`)
- ✅ Keyboard focus management (`focus_manager`)
- ✅ Minimal + Basic catalog assembly (`catalogs`)

### Optional features

| Feature | Description | Enable |
|------|------|------|
| `audio` | Real audio playback via `rodio` (background thread; local file paths only; requires the ALSA system library) | `--features audio` |

> There is no video feature — terminals have no mature TUI video story, so it always renders a placeholder.

## Modules

| Module | Responsibility |
|------|------|
| `surface` | Recursive render entry (`SurfaceRenderer`) |
| `component_impl` | `TuiComponent` trait + registration |
| `layout_engine` | Weight-based split / alignment |
| `focus_manager` | Keyboard focus management |
| `components` | The 18 component implementations |
| `catalogs` | Minimal + Basic catalog assembly |

## Usage

```bash
cargo add a2ui-base a2ui-tui
```

```rust
use a2ui_base::message_processor::MessageProcessor;
use a2ui_tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui_tui::surface::SurfaceRenderer;

let catalog = build_basic_catalog();
let registry = build_basic_registry();
let mut processor = MessageProcessor::new(vec![catalog]);

processor.process_message(MessageProcessor::parse_message(json)?)?;

let surface = processor.model.get_surface("main").unwrap();
let renderer = SurfaceRenderer::new(surface, &registry, &catalog);
renderer.render(&mut frame, area);
```

> Via the umbrella, swap `a2ui_base::` / `a2ui_tui::` for `a2ui::core::` / `a2ui::tui::`.

## License

MIT
