# A2UI — Ratatui-based TUI Renderer

English | [中文](README.md)

A Rust implementation of the [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) protocol terminal renderer, built on [ratatui](https://ratatui.rs/).

A2UI is a JSON-based streaming UI protocol that allows AI Agents to dynamically generate and update terminal user interfaces.

## Features

- ✅ Full A2UI v1.0 protocol support
- ✅ **18 TUI components**: Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
- ✅ **14 client-side functions**: required, regex, length, numeric, email, and/or/not, formatString, formatNumber, formatCurrency, formatDate, pluralize, openUrl
- ✅ Modular layered architecture (Core Layer + TUI Layer)
- ✅ JSON Pointer data binding with reactive state management
- ✅ Gallery App sample browser with progressive message rendering
- ✅ 89 unit/integration tests, including end-to-end tests with A2UI specification examples

## Screenshots

**Gallery Sample Browser**

![Gallery](screenshot/gallery.png)

**Login Form**

![Login Form](screenshot/login-form.png)

## Quick Start

```bash
# Run the Gallery App
cargo run
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
