# A2UI — Rust impl of the A2UI protocol (ratatui terminal + Slint / egui / Bevy / Iced desktop)

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

English | [中文](README.md)

A Rust implementation of the [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) protocol — a JSON-based streaming UI protocol that lets AI Agents dynamically generate and update interfaces.

On top of a single framework-agnostic core (`a2ui-base`), it ships **5 rendering backends**: the default terminal backend `a2ui-tui` (built on [ratatui](https://ratatui.rs/)), plus four **optional** native-desktop backends — [Slint](https://slint.dev/), [egui](https://github.com/emilk/egui), [Bevy](https://bevyengine.org), and [Iced](https://github.com/iced-rs/iced). See the [Backend Support Matrix](#backend-support-matrix) for each backend's rendering fidelity and real-input capability.

The project is organized as a Cargo workspace: `a2ui-base` (framework-agnostic core) + 5 backends (`a2ui-tui` / `a2ui-slint` / `a2ui-egui` / `a2ui-bevy` / `a2ui-iced`) + a matching `*-gallery` demo app for each + `a2ui` (an umbrella that re-exports them, keeping `use a2ui::core::...` / `use a2ui::tui::...` paths working).

## Features

- ✅ Full A2UI v1.0 protocol support
- ✅ **18 TUI components**: Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
  - Interactive: Text, Row, Column, Button, TextField, Slider, CheckBox, ChoicePicker, DateTimeInput (arrow keys adjust the value)
  - Placeholders (default): Image / Video / AudioPlayer render only text placeholders (`[🖼 description]`, `[▶ url]`, `[♫ url]`) — the terminal cannot decode pixels/audio/video. Enable real image/audio rendering via the Optional Features below.
- ✅ **Capabilities negotiation**: `ClientCapabilities` / `ServerCapabilities` types + a builder that derives `supportedCatalogIds` from registered catalogs.
- ✅ **Inline catalogs**: the server can declare `acceptsInlineCatalogs`; the client parses and validates inline catalog JSON (UAX#31 identifier checks) and registers schema-only functions at runtime.
- ✅ **Generic fallback renderer**: unknown / inline-custom component types render as a visible labeled box (type + properties + children) instead of a bare "unknown" error.
- ✅ **14 client-side functions**: required, regex, length, numeric, email, and/or/not, formatString, formatNumber, formatCurrency, formatDate, pluralize, openUrl
- ✅ **Payload validation**: integrity / topology / recursion & path checks plus a fault-tolerant `parse_and_fix` (auto-heals malformed JSON like smart quotes and trailing commas), ported from the Python SDK. Opt-in on `MessageProcessor` (`with_validation(cfg)` + `drain_validation()`), OFF by default and never blocks component loading — for untrusted or LLM-generated payloads.
- ✅ **Modular Cargo workspace architecture** (`a2ui-base` framework-agnostic / `a2ui-tui` ratatui backend / `a2ui-gallery` demo app / `a2ui` umbrella)
- ✅ JSON Pointer data binding with reactive state management
- ✅ Gallery App sample browser with progressive message rendering
- ✅ **219 unit/integration tests** (core 127 + tui 61 + gallery e2e 21 + slint 10), including end-to-end tests with A2UI specification examples

## Screenshots

**Gallery Sample Browser**

![Gallery](screenshot/gallery.png)

**Login Form**

![Login Form](screenshot/login-form.png)

**Agent Chat** (AI chat interface, `08_agent_chat` example: multi-surface chat layout, streaming A2UI messages, rich components like Card / Column / Row / Divider)

![Agent Chat](screenshot/agent-chat.png)

**Invitation Builder** (spec sample `30_live-invitation-builder`: a reactive form layout where TextField / Slider / ChoicePicker / DateTimeInput components work together to preview an invitation in real time)

![Invitation Builder](screenshot/invitation-builder.png)

**Sci-fi HUD — backend comparison** (same data, same `updateDataModel` protocol, different renderer; every live value — gauges, radar sweep, event log — is read from the a2ui data model)

| ratatui terminal (`17_scifi_hud` in `a2ui`) | Iced desktop (`17_scifi_hud` in `a2ui-iced`) |
|:---:|:---:|
| ![Sci-fi HUD — ratatui](screenshot/sci-fi-hud-tui.png) | ![Sci-fi HUD — Iced](screenshot/sci-fi-hud-iced.png) |

The ratatui version (left) uses custom `TuiComponent`s to draw ASCII gauges and a character-grid radar; the Iced version (right) uses `progress_bar` gauges and a `Canvas`-drawn radar sweep, rendered into a native window. The architecture is identical — only **data** flows through the protocol; the rendering layer is each backend's own.

> The sci-fi HUD is currently realized for the **ratatui (TUI)** and **Iced** backends; the Slint / egui / Bevy galleries render the standard spec samples and do not yet have a HUD variant.

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
┌──────────────────────────────────────────────────────────────┐
│  apps:       a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   a2ui-bevy-gallery   a2ui-iced-gallery (desktop)
├──────────────────────────────────────────────────────────────┤
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced])
├──────────────────────────────────────────────────────────────┤
│  backends:   a2ui-tui (ratatui)   a2ui-slint (Slint, opt-in)   a2ui-egui (egui, opt-in)   a2ui-bevy (Bevy, opt-in)   a2ui-iced (Iced, opt-in)
├──────────────────────────────────────────────────────────────┤
│  a2ui-base (framework-agnostic: Protocol / Model / Catalog / Processor)
└──────────────────────────────────────────────────────────────┘
```

Dependencies flow upward: `a2ui-base` underpins five backends — `a2ui-tui` (ratatui, default), `a2ui-slint` (Slint desktop, optional), `a2ui-egui` (egui desktop, optional), `a2ui-bevy` (Bevy ECS UI desktop, optional), and `a2ui-iced` (Iced desktop, optional). Each backend has a matching `*-gallery` app; the `a2ui` umbrella depends on core + tui (slint / egui / bevy / iced each behind a same-named feature). `a2ui-base` has zero ratatui/slint/egui/bevy/iced dependency and can be used standalone by other backends.

## Backend Support Matrix

All five backends share the same `a2ui-base` core (interaction logic / `dispatch_event` / `apply_event_result`), but rendering fidelity and "real input" capability vary by GUI framework:

> ✅ Full (rendered; interactive controls accept input) · 🟡 Best-effort (read-only / limited interaction) · ⬜ Placeholder

| Component | TUI (ratatui) | Slint | egui | Bevy | Iced |
|-----------|:---:|:---:|:---:|:---:|:---:|
| Text / Row / Column / Card / List / Divider | ✅ | ✅ | ✅ | ✅ | ✅ |
| Button | ✅ | ✅ | ✅ | ✅ | ✅ |
| Modal | ✅ | ✅ | ✅ | ✅ | ✅ |
| TextField | ✅ | 🟡 | ✅ | ✅ | ✅ |
| CheckBox | ✅ | ✅ | ✅ | ✅ | ✅ |
| Slider | ✅ | 🟡 | ✅ | ✅ | ✅ |
| ChoicePicker | ✅ | 🟡 | ✅ | ⬜ | ✅ |
| Tabs | ✅ | 🟡 | 🟡 | 🟡 | 🟡 |
| DateTimeInput | ✅ | 🟡 | 🟡 | 🟡 | 🟡 |
| Icon | ✅ | 🟡 | 🟡 | 🟡 | 🟡 |
| Image | ✅² | ⬜ | ⬜ | ⬜ | ⬜ |
| Video | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |
| AudioPlayer | ✅¹ | ⬜ | ⬜ | ⬜ | ⬜ |

¹ Needs the `audio` feature.
² The TUI backend decodes and renders actual image pixels via `ratatui-image` (kitty / iTerm2 / Sixel / Halfblocks auto-degrade, local paths only); the four desktop backends currently render only a text placeholder.

- **The TUI backend is the reference implementation** — all 18 components render fully; real images are on by default (`ratatui-image`), audio needs the `audio` feature, video is always a placeholder.
- **Genuine input on the interactive widgets (TextField / Slider / CheckBox / ChoicePicker)**: full on egui, Bevy, Iced, and TUI. **On Slint only Button / CheckBox clicks are wired** — TextField and Slider render read-only.
- **Bevy's ChoicePicker** is currently a text label (`[ChoicePicker: …]`); not yet wired to a native picker.
- **Iced is the cleanest-mapping backend** (no state bridge, no diffing); all five interactive widgets are native.

## Slint Desktop Backend

Alongside the ratatui terminal backend, the project ships **`a2ui-slint`**, which renders A2UI component trees into a **native desktop window** (built on [Slint](https://slint.dev/), pinned to 1.16). The framework-agnostic interaction logic (focus traversal, event dispatch, `EventResult` application) is shared in `a2ui-base`, so both backends behave identically for keyboard / button interactions.

**It is opt-in and heavy**: `a2ui-slint` is a **non-default workspace member** (it pulls the Slint toolchain + GUI system libraries). A plain `cargo build` only compiles the ratatui stack. Build the Slint backend explicitly:

```bash
cargo build -p a2ui-slint --features backend
```

The umbrella crate also re-exports it as `a2ui::slint` behind a `slint` cargo feature.

### Running the Gallery (desktop)

`a2ui-slint-gallery` loads the same embedded A2UI samples as the ratatui gallery, in a window. It prints the full numbered sample list at startup:

```bash
cargo run -p a2ui-slint-gallery             # first sample
cargo run -p a2ui-slint-gallery -- 3        # by 1-based index
cargo run -p a2ui-slint-gallery -- login    # by case-insensitive name substring
```

Renderer: `renderer-software` + `backend-winit` — it works **without a GPU / OpenGL driver**.

### Component coverage

All 18 A2UI component kinds render:

- **Rich**: Text / Button / Column / Row / Card / TextField / CheckBox / Slider (Button & CheckBox clicks dispatch through the shared `core::components::dispatch_event`)
- **Best-effort**: Divider / Icon / Tabs / Modal / List / ChoicePicker / DateTimeInput
- **Placeholders**: Image / Video / AudioPlayer render as labeled placeholders (binary media isn't carried into the Slint tree)

### Implementation note: why the tree is flattened

Slint **cannot express recursion** (neither recursive structs nor self-referencing components — see [slint-ui/slint#4218](https://github.com/slint-ui/slint/issues/4218)). So instead of a nested tree, `live_tree` flattens the component tree into a `Vec<LiveNode>` with index-based `children`, and `build.rs` code-generates a **bounded-depth** component chain `Node0` (leaf) → … → `Node7` (root). A2UI trees are shallow, so depth 7 covers realistic UIs; deeper subtrees truncate to a `…`. This is the key constraint a future contributor needs to know.

### Current limitations

- Trees deeper than 7 levels truncate;
- TextField shows its value but isn't wired to a native editable input yet;
- Tabs / ChoicePicker / DateTimeInput render, but their keyboard handlers aren't in the shared core dispatch (interaction beyond Button / CheckBox is not yet wired on the Slint side).

## Iced Desktop Backend

The project also ships **`a2ui-iced`**, which renders A2UI component trees into a **native desktop window** (built on [Iced](https://github.com/iced-rs/iced), pinned to 0.14). **This is the cleanest mapping of the five backends** — Iced is Elm: `view(&state)` returns an immutable `Element` tree and `update(&mut state, msg)` mutates state. So interactive widgets read straight from the data model in `view` and write back through a `Message` in `update`: no egui-style `EditBuffers` state bridge and no bevy-style reconciler. **No state bridge, no diffing.** Button presses reuse the shared `core::components::dispatch_event` + `apply_event_result`; Modals float as a centered overlay layered via a `Stack`.

**It is an optional dependency**: `a2ui-iced` is a **non-default workspace member** (it pulls wgpu + winit). Plain `cargo build` only compiles the ratatui stack — build it explicitly:

```bash
cargo build -p a2ui-iced --features backend
```

The umbrella crate re-exports the backend as `a2ui::iced` under the `iced` cargo feature. The renderer defaults to wgpu (GPU), with a tiny-skia software fallback.

### Run the Gallery (Iced)

`a2ui-iced-gallery` loads the same embedded A2UI samples:

```bash
cargo run -p a2ui-iced-gallery             # the first sample
cargo run -p a2ui-iced-gallery -- 3        # by 1-based index
cargo run -p a2ui-iced-gallery -- login    # by case-insensitive name substring
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
| `13_image` | Real image rendering (kitty / iTerm2 / Sixel / Halfblocks auto-degrade) | `cargo run -p a2ui --example 13_image` |
| `14_audio` | Interactive AudioPlayer (needs the `audio` feature) | `cargo run -p a2ui --example 14_audio` |
| `15_date_time_input` | Interactive DateTimeInput | `cargo run -p a2ui --example 15_date_time_input` |
| `16_custom_component` | Custom component — implementing the `TuiComponent` trait | `cargo run -p a2ui --example 16_custom_component` |
| `17_scifi_hud` | a2ui-driven cyberpunk HUD (see screenshot above) | `cargo run -p a2ui --example 17_scifi_hud` |
| `18_validate` | Payload validation: integrity / topology / `parse_and_fix`, STRICT vs RELAXED | `cargo run -p a2ui --example 18_validate` |

> 20 examples in total (including the `07b` / `07c` debug variants) — full list in `crates/a2ui/examples/`.

## Optional Features

Image rendering is **built-in and on by default**: a plain `cargo build` renders real images via `ratatui-image` (auto-degrading kitty / iTerm2 / Sixel / Halfblocks), local file paths only, falling back to the placeholder when unloadable. The following are additional **opt-in** features, OFF by default:

> The desktop GUI backend lives in its own [Slint Desktop Backend](#slint-desktop-backend) section above (a separate workspace member, not a ratatui feature).

| Feature | Description | Enable | Limitation |
|---------|-------------|--------|------------|
| `audio` | Real audio playback via `rodio` (background thread) | `--features audio` | **LOCAL file paths only**; requires the ALSA system dev library (`alsa-lib-devel` on Fedora / `libasound2-dev` on Debian); silently falls back to the placeholder on failure |
| — (Video) | No feature exists for video | — | There is no mature TUI video solution, so Video always renders a placeholder |

## Using as a Library

`a2ui-base` is fully framework-agnostic — usable on its own for non-ratatui scenarios, or as the foundation for other backends (the project already builds the [Slint desktop backend](#slint-desktop-backend) on top of it):

```bash
# Option 1: depend directly (most minimal, recommended for libraries)
cargo add a2ui-base a2ui-tui

# Option 2: via the umbrella (keeps a2ui:: paths)
cargo add a2ui
```

```rust
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::catalog::Catalog;
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

> Via the umbrella, just swap `a2ui_base::` / `a2ui_tui::` for `a2ui::core::` / `a2ui::tui::` — everything else stays the same.

## License

MIT
