# a2ui-base

[![crates.io](https://img.shields.io/crates/v/a2ui-base.svg)](https://crates.io/crates/a2ui-base)
[![docs.rs](https://docs.rs/a2ui-base/badge.svg)](https://docs.rs/a2ui-base)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · framework-agnostic core layer
>
> This crate is the foundational sub-crate of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

The **framework-agnostic core** of the [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) protocol: protocol types, component / data models, catalogs, the message processor, capabilities negotiation, validation, and the interaction layer shared by every UI backend. It **depends on no UI framework** (no ratatui / Slint / egui), so it can be used standalone for other backends or pure protocol parsing.

## Where it fits

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:  a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
├───────────────────────────────────────────────────────────────────────┤
│  ▶ a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor) │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-base` is the foundation of the whole workspace — all three backends (ratatui / Slint / egui) are built on it. The framework-agnostic interaction logic (focus traversal `focus`, event-result application `interaction`, component behavior `components`) lives here so every backend agrees on keyboard / button behavior.

## Modules

| Module | Responsibility |
|------|------|
| `protocol` | A2UI v1.0 JSON message types (server→client, client→server) |
| `model` | Runtime component tree, surfaces, JSON Pointer data binding |
| `catalog` | Catalog, component API, function implementations, schema-only functions, inline catalogs |
| `message_processor` | Message parsing → state changes; `process_message` / `parse_jsonl` |
| `capabilities` | `ClientCapabilities` / `ServerCapabilities` negotiation + inline catalog parsing (UAX#31 validation) |
| `validate` | Protocol validation (`ValidationConfig` / `ValidationReport`) |
| `observable` | Reactive state management |
| `focus` / `interaction` / `components` | Shared interaction layer (focus traversal, `EventResult` application, component `handle_event`) |

## Usage

```bash
cargo add a2ui-base
```

```rust
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::catalog::Catalog;

let mut processor = MessageProcessor::new(vec![/* catalogs */]);

// Parse and process one JSON message
let msg = MessageProcessor::parse_message(r#"{"version":"v1.0",...}"#)?;
processor.process_message(msg)?;

// Read protocol-produced outgoing messages (functionResponse / actionResponse / ...)
let outgoing = processor.drain_outgoing();
```

> Want a renderable terminal UI out of the box? Combine it with [`a2ui-tui`](https://crates.io/crates/a2ui-tui); or reach this crate as `a2ui::core::...` via the umbrella [`a2ui`](https://crates.io/crates/a2ui).

## License

MIT
