# a2ui-iced

[![crates.io](https://img.shields.io/crates/v/a2ui-iced.svg)](https://crates.io/crates/a2ui-iced)
[![docs.rs](https://docs.rs/a2ui-iced/badge.svg)](https://docs.rs/a2ui-iced)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · Iced Elm-architecture desktop backend (optional)
>
> This crate is the fifth rendering backend of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

Renders an [A2UI](https://github.com/a2ui-project/a2ui) component tree into a **native desktop window**, built on [Iced](https://github.com/iced-rs/iced) (Elm architecture, pinned to 0.14). Of the five backends this is the **cleanest** mapping: Iced is Elm — `view(&state)` returns an immutable `Element` tree and `update(&mut state, msg)` mutates state. So interactive widgets **read straight from the data model** in `view` and **write back through a `Message`** in `update` — no egui-style `EditBuffers` state bridge (immediate mode borrows the data model for the whole frame) and no bevy-style reconciler (retained ECS must diff/patch the entity tree). **No state bridge, no diffing.** Button presses reuse the shared `core::components::dispatch_event` + `apply_event_result`, like every other backend.

> **Optional dependency:** this crate is a **non-default workspace member** (it pulls wgpu + winit); plain `cargo build` does not compile it.

## Where it fits

```
┌────────────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced}-gallery          │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced]) │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui   a2ui-bevy   a2ui-iced │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor)  │
└────────────────────────────────────────────────────────────────────────────┘
```

`a2ui-iced` depends on [`a2ui-base`](https://crates.io/crates/a2ui-base); it is depended on by `a2ui-iced-gallery` and (under the `iced` feature) the umbrella `a2ui`.

## Build

Everything lives behind the `backend` cargo feature, which pulls in the Iced runtime (wgpu renderer + winit window). Without it this crate is an empty shell (no dependencies beyond `a2ui-base`), keeping the workspace's default build light.

```bash
cargo build -p a2ui-iced --features backend
```

The renderer defaults to wgpu (GPU), with a tiny-skia software fallback.

## Why no state bridge is needed (implementation note)

Iced is Elm: `view(&self)` borrows the surface's data model / components (read-only) and builds an element tree that **owns its data** (`text(String)`, `text_input(placeholder, value)`, and friends all **copy** the `&str` they receive into owned storage; the returned element's `'a` lifetime is bound only to the `on_*` closures, which capture owned `Message` values). User interaction is expressed as a `Message` attached to a widget, and `update(&mut self, msg)` applies it once `view`'s borrows are dropped.

Because `view` and `update` never overlap, there is **no** egui-style "collect-then-apply" `PendingInteraction` buffer and **no** `EditBuffers` with a seed/detect/writeback lifecycle. The `Message` stream *is* the interaction bridge. This is the least bookkeeping of all five backends.

## Modules

| Module | Responsibility |
|--------|----------------|
| `walker` | Recursively builds the A2UI component tree into an `Element` tree (pure — returns an owned tree) |
| `app` | `IcedApp` — owns surface state, provides the Elm `view`/`update` pair |
| `components` | The Iced implementation of each A2UI component (real native widgets) |
| `message` | `Message` — the Elm interaction channel widgets produce and `update` consumes |

## License

MIT
