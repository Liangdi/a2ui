# A2UI — Ratatui-based TUI Renderer

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

English | [中文](README.md)

A Rust implementation of the [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) protocol terminal renderer, built on [ratatui](https://ratatui.rs/).

A2UI is a JSON-based streaming UI protocol that allows AI Agents to dynamically generate and update terminal user interfaces.

## Features

- ✅ Full A2UI v1.0 protocol support
- ✅ **18 TUI components**: Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
  - Interactive: Text, Row, Column, Button, TextField, Slider, CheckBox, ChoicePicker, DateTimeInput (arrow keys adjust the value)
  - Placeholders (default): Image / Video / AudioPlayer render only text placeholders (`[🖼 description]`, `[▶ url]`, `[♫ url]`) — the terminal cannot decode pixels/audio/video. Enable real image/audio rendering via the Optional Features below.
- ✅ **Capabilities negotiation**: `ClientCapabilities` / `ServerCapabilities` types + a builder that derives `supportedCatalogIds` from registered catalogs.
- ✅ **Inline catalogs**: the server can declare `acceptsInlineCatalogs`; the client parses and validates inline catalog JSON (UAX#31 identifier checks) and registers schema-only functions at runtime.
- ✅ **Generic fallback renderer**: unknown / inline-custom component types render as a visible labeled box (type + properties + children) instead of a bare "unknown" error.
- ✅ **14 client-side functions**: required, regex, length, numeric, email, and/or/not, formatString, formatNumber, formatCurrency, formatDate, pluralize, openUrl
- ✅ Modular layered architecture (Core Layer + TUI Layer)
- ✅ JSON Pointer data binding with reactive state management
- ✅ Gallery App sample browser with progressive message rendering
- ✅ **150 unit/integration tests** (core 102 + tui 48), including end-to-end tests with A2UI specification examples

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
cargo run

# Minimal capabilities-handshake demo (no TUI)
cargo run --example 12_handshake
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
┌─────────────────────────────────────┐
│  Gallery App (main.rs)              │  ← Demo application
├─────────────────────────────────────┤
│  TUI Layer (src/tui/)              │  ← ratatui component impls
│  Surface, Components, Catalogs     │
├─────────────────────────────────────┤
│  Core Layer (src/core/)            │  ← Framework-agnostic
│  Protocol, Models, Catalog,        │
│  MessageProcessor, Observable      │
└─────────────────────────────────────┘
```

### Project Structure

```
src/
├── lib.rs                    # Crate root
├── main.rs                   # Gallery App entry point
├── core/                     # Framework-agnostic layer
│   ├── error.rs              # Error types
│   ├── protocol/             # A2UI protocol types
│   │   ├── common_types.rs   # DynamicString, FunctionCall, ChildList...
│   │   ├── server_to_client.rs
│   │   └── client_to_server.rs
│   ├── model/                # State models
│   │   ├── data_model.rs     # JSON Pointer data store
│   │   ├── component_model.rs
│   │   ├── surface_model.rs
│   │   ├── data_context.rs   # Scoped data access + dynamic value resolution
│   │   └── ...
│   ├── catalog/              # Catalog system
│   │   ├── catalog.rs        # Catalog component/function registry
│   │   ├── basic_functions.rs # 14 Basic Catalog functions
│   │   └── ...
│   ├── observable/           # EventStream, Signal
│   └── message_processor.rs  # JSON parse → state mutation
├── tui/                      # ratatui rendering layer
│   ├── surface.rs            # Recursive rendering entry point
│   ├── component_impl.rs     # TuiComponent trait
│   ├── layout_engine.rs      # Weighted split / alignment
│   ├── focus_manager.rs      # Keyboard focus management
│   ├── components/           # 18 component implementations
│   └── catalogs/             # Minimal + Basic Catalog assembly
└── gallery/                  # Gallery sample application
    ├── app.rs                # Main event loop
    └── sample_loader.rs      # Load JSON samples
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
| `01_hello_world` | Simplest A2UI program | `cargo run --example 01_hello_world` |
| `02_jsonl_stream` | JSONL stream processing & progressive rendering | `cargo run --example 02_jsonl_stream` |
| `03_data_binding` | JSON Pointer reactive data binding | `cargo run --example 03_data_binding` |
| `04_login_form` | Full form: inputs, validation, focus, actions | `cargo run --example 04_login_form` |
| `05_custom_function` | Custom catalog function implementation | `cargo run --example 05_custom_function` |
| `06_call_function` | Server-initiated `callFunction` & `functionResponse` | `cargo run --example 06_call_function` |
| `07_action_response` | `actionResponse` with `responsePath` reactive updates | `cargo run --example 07_action_response` |
| `12_handshake` | Capabilities-negotiation handshake | `cargo run --example 12_handshake` |

## Optional Features

Image rendering is **built-in and on by default**: a plain `cargo build` renders real images via `ratatui-image` (auto-degrading kitty / iTerm2 / Sixel / Halfblocks), local file paths only, falling back to the placeholder when unloadable. The following are additional **opt-in** features, OFF by default:

| Feature | Description | Enable | Limitation |
|---------|-------------|--------|------------|
| `audio` | Real audio playback via `rodio` (background thread) | `--features audio` | **LOCAL file paths only**; requires the ALSA system dev library (`alsa-lib-devel` on Fedora / `libasound2-dev` on Debian); silently falls back to the placeholder on failure |
| — (Video) | No feature exists for video | — | There is no mature TUI video solution, so Video always renders a placeholder |

## Using as a Library

```rust
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::catalog::Catalog;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

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

## License

MIT
