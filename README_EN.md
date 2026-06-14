# A2UI — Ratatui-based TUI Renderer

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

English | [中文](README.md)

A Rust implementation of the [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) protocol terminal renderer, built on [ratatui](https://ratatui.rs/).

A2UI is a JSON-based streaming UI protocol that allows AI Agents to dynamically generate and update terminal user interfaces.

The project is organized as a Cargo workspace: `a2ui-core` (framework-agnostic core) + `a2ui-tui` (ratatui backend) + `a2ui-gallery` (demo app) + `a2ui` (umbrella that re-exports core+tui, keeping `use a2ui::core::...` / `use a2ui::tui::...` paths working).

## Features

- ✅ Full A2UI v1.0 protocol support
- ✅ **18 TUI components**: Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
  - Interactive: Text, Row, Column, Button, TextField, Slider, CheckBox, ChoicePicker, DateTimeInput (arrow keys adjust the value)
  - Placeholders (default): Image / Video / AudioPlayer render only text placeholders (`[🖼 description]`, `[▶ url]`, `[♫ url]`) — the terminal cannot decode pixels/audio/video. Enable real image/audio rendering via the Optional Features below.
- ✅ **Capabilities negotiation**: `ClientCapabilities` / `ServerCapabilities` types + a builder that derives `supportedCatalogIds` from registered catalogs.
- ✅ **Inline catalogs**: the server can declare `acceptsInlineCatalogs`; the client parses and validates inline catalog JSON (UAX#31 identifier checks) and registers schema-only functions at runtime.
- ✅ **Generic fallback renderer**: unknown / inline-custom component types render as a visible labeled box (type + properties + children) instead of a bare "unknown" error.
- ✅ **14 client-side functions**: required, regex, length, numeric, email, and/or/not, formatString, formatNumber, formatCurrency, formatDate, pluralize, openUrl
- ✅ **Modular Cargo workspace architecture** (`a2ui-core` framework-agnostic / `a2ui-tui` ratatui backend / `a2ui-gallery` demo app / `a2ui` umbrella)
- ✅ JSON Pointer data binding with reactive state management
- ✅ Gallery App sample browser with progressive message rendering
- ✅ **173 unit/integration tests** (core 83 + tui 69 + gallery e2e 21), including end-to-end tests with A2UI specification examples

## Screenshots

**Gallery Sample Browser**

![Gallery](screenshot/gallery.png)

**Login Form**

![Login Form](screenshot/login-form.png)

**Agent Chat** (AI chat interface, `08_agent_chat` example: multi-surface chat layout, streaming A2UI messages, rich components like Card / Column / Row / Divider)

![Agent Chat](screenshot/agent-chat.png)

**Invitation Builder** (spec sample `30_live-invitation-builder`: a reactive form layout where TextField / Slider / ChoicePicker / DateTimeInput components work together to preview an invitation in real time)

![Invitation Builder](screenshot/invitation-builder.png)

**Sci-fi HUD** (cyberpunk tactical HUD, `17_scifi_hud` example: custom `TuiComponent` panels compose telemetry / radar / event-log, with all live data — gauges, sweep, events — driven through the a2ui `updateDataModel` protocol)

![Sci-fi HUD](screenshot/sci-fi-hud.png)

## Quick Start

```bash
# Run the Gallery App
cargo run -p a2ui-gallery

# Install the Gallery App (provides the `a2ui_gallery` binary)
cargo install a2ui-gallery

# Run an example (lives in the umbrella crate)
cargo run -p a2ui --example 12_handshake
```

### Controls

| Key | Action |
|-----|--------|
| `↑`/`k`, `↓`/`j` | Navigate sample list |
| `Enter` | Select and render current sample |
| `n` | Step through next message |
| `a` | Process all remaining messages |
| `r` | Reset and replay |
| `Tab` | Cycle focus |
| `Esc` | Back to list / Quit |

## Architecture

```
┌─────────────────────────────────────────┐
│  a2ui-gallery (bin)                     │  ← Gallery App + spec tree embedded at build time
├─────────────────────────────────────────┤
│  a2ui (umbrella lib)                    │  ← re-exports core+tui, keeps use a2ui:: paths
├─────────────────────────────────────────┤
│  a2ui-tui  (ratatui backend)            │  ← 18 component impls + Surface rendering
├─────────────────────────────────────────┤
│  a2ui-core (framework-agnostic)         │  ← Protocol / Model / Catalog / Processor
└─────────────────────────────────────────┘
```

Dependencies flow upward: `a2ui-core` ← `a2ui-tui` ← `a2ui-gallery`; the `a2ui` umbrella depends on core+tui. `a2ui-core` has zero ratatui dependency and can be used standalone by other backends.

### Project Structure

```
crates/
├── core/              # a2ui-core: framework-agnostic layer
│   └── src/
│       ├── protocol/ model/ catalog/ observable/
│       ├── message_processor.rs   # JSON parse → state mutation
│       ├── capabilities.rs        # Capabilities negotiation + inline-catalog parsing
│       └── error.rs event.rs
├── tui/               # a2ui-tui: ratatui rendering layer
│   └── src/
│       ├── surface.rs             # Recursive rendering entry point
│       ├── component_impl.rs      # TuiComponent trait + registry
│       ├── layout_engine.rs       # Weighted split / alignment
│       ├── focus_manager.rs       # Keyboard focus management
│       ├── components/            # 18 component implementations
│       └── catalogs/              # Minimal + Basic Catalog assembly
├── gallery/           # a2ui-gallery: Gallery App (bin + lib)
│   ├── src/                       # app.rs / sample_loader.rs / main.rs
│   ├── tests/e2e.rs               # End-to-end tests (loads spec samples)
│   └── a2ui/specification/        # Spec tree embedded at build time
└── a2ui/              # a2ui: umbrella, re-exports core+tui
    ├── src/lib.rs
    └── examples/                  # 17 examples
```

## Protocol Overview

A2UI uses a JSON streaming message format to drive UI rendering:

```jsonl
{"version":"v1.0","createSurface":{"surfaceId":"main","catalogId":"https://a2ui.org/.../catalog.json"}}
{"version":"v1.0","updateComponents":{"surfaceId":"main","components":[...]}}
{"version":"v1.0","updateDataModel":{"surfaceId":"main","path":"/user/name","value":"Alice"}}
{"version":"v1.0","deleteSurface":{"surfaceId":"main"}}
```

## Examples

| Example | Description | Run |
|---------|-------------|-----|
| `01_hello_world` | Simplest A2UI program | `cargo run -p a2ui --example 01_hello_world` |
| `02_jsonl_stream` | JSONL stream processing & progressive rendering | `cargo run -p a2ui --example 02_jsonl_stream` |
| `03_data_binding` | JSON Pointer reactive data binding | `cargo run -p a2ui --example 03_data_binding` |
| `04_login_form` | Full form: inputs, validation, focus, actions | `cargo run -p a2ui --example 04_login_form` |
| `05_custom_function` | Custom catalog function implementation | `cargo run -p a2ui --example 05_custom_function` |
| `06_call_function` | Server-initiated `callFunction` & `functionResponse` | `cargo run -p a2ui --example 06_call_function` |
| `07_action_response` | `actionResponse` with `responsePath` reactive updates | `cargo run -p a2ui --example 07_action_response` |
| `12_handshake` | Capabilities-negotiation handshake | `cargo run -p a2ui --example 12_handshake` |

## Optional Features

Image rendering is **built-in and on by default**: a plain `cargo build` renders real images via `ratatui-image` (auto-degrading kitty / iTerm2 / Sixel / Halfblocks), local file paths only, falling back to the placeholder when unloadable. The following are additional **opt-in** features, OFF by default:

| Feature | Description | Enable | Limitation |
|---------|-------------|--------|------------|
| `audio` | Real audio playback via `rodio` (background thread) | `--features audio` | **LOCAL file paths only**; requires the ALSA system dev library (`alsa-lib-devel` on Fedora / `libasound2-dev` on Debian); silently falls back to the placeholder on failure |
| — (Video) | No feature exists for video | — | There is no mature TUI video solution, so Video always renders a placeholder |

## Using as a Library

`a2ui-core` is fully framework-agnostic — usable on its own for non-ratatui scenarios, or as the foundation for other backends (e.g. a planned slint backend):

```bash
# Option 1: depend directly (most minimal, recommended for libraries)
cargo add a2ui-core a2ui-tui

# Option 2: via the umbrella (keeps a2ui:: paths)
cargo add a2ui
```

```rust
use a2ui_core::message_processor::MessageProcessor;
use a2ui_core::catalog::Catalog;
use a2ui_tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui_tui::surface::SurfaceRenderer;

// Create processor with Basic Catalog
let catalog = build_basic_catalog();
let registry = build_basic_registry();
let mut processor = MessageProcessor::new(vec![catalog]);

// Parse and process messages
let msg = MessageProcessor::parse_message(r#"{"version":"v1.0","createSurface":{...}}"#)?;
processor.process_message(msg)?;

// Render (within a ratatui Frame)
let surface = processor.model.get_surface("main").unwrap();
let renderer = SurfaceRenderer::new(surface, &registry, &catalog);
renderer.render(&mut frame, area);
```

> Via the umbrella, just swap `a2ui_core::` / `a2ui_tui::` for `a2ui::core::` / `a2ui::tui::` — everything else stays the same.

## License

MIT
