# a2ui-bevy

[![crates.io](https://img.shields.io/crates/v/a2ui-bevy.svg)](https://crates.io/crates/a2ui-bevy)
[![docs.rs](https://docs.rs/a2ui-bevy/badge.svg)](https://docs.rs/a2ui-bevy)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · Bevy ECS UI backend (optional)
>
> This crate is the fourth rendering backend of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

Translates an [A2UI](https://github.com/a2ui-project/a2ui) component tree into a **retained Bevy UI entity tree**, built on [Bevy](https://bevyengine.org) 0.18's ECS UI stack. Unlike the [egui](https://crates.io/crates/a2ui-egui) backend (immediate-mode — rebuilds every frame and carries widget state in an `EditBuffers` map), Bevy is **retained-mode ECS**: widgets are entities that live across frames. Because Bevy's interactive widgets (`bevy_ui_widgets` Button / Checkbox / Slider and the external `bevy_ui_text_input`) only keep correct drag / hover / focus / cursor state when their **entity identity is preserved across frames**, this backend introduces a **React-style reconciler** — it keeps a stable `HashMap<component_id, Entity>` and spawn / update / despawn / reorder incrementally each frame. Since the text-input entity persists, it owns its own cursor and edit state, so **no `EditBuffers` map is needed**. Button / value-change interactions reuse the shared `core::components::dispatch_event` + `apply_event_result`, identical to the other backends.

> **Optional dependency**: this crate is a **non-default workspace member** (it pulls in Bevy's wgpu + winit toolchain); a plain `cargo build` does not compile it.

## Where it fits

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   a2ui-bevy-gallery   a2ui-iced-gallery│
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced])                              │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui   a2ui-bevy   a2ui-iced                          │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor)                                    │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

`a2ui-bevy` depends on [`a2ui-base`](https://crates.io/crates/a2ui-base); it is consumed by `a2ui-bevy-gallery` and (under the `bevy` feature) the umbrella `a2ui`.

## Building

Everything lives behind the `backend` cargo feature, which is what pulls in the Bevy runtime + `bevy_ui_text_input`. Without that feature the crate is an empty shell (no dependencies beyond `a2ui-base`), keeping the workspace's default build light.

```bash
cargo build -p a2ui-bevy --features backend
```

The renderer uses wgpu; it needs a GPU/wgpu stack but no game-specific tooling.

## The reconciler (implementation note)

Bevy's interactive widgets only behave when their entities survive from frame to frame — a per-frame rebuild (the Slint / egui approach) would fling sliders and drop text cursors every frame. So the reconciler does a two-pass diff/patch against `A2uiState`'s stable `node_map: HashMap<component_id, Entity>`:

1. **Plan** (read-only pass over the A2UI model) — collect a `PlanNode` for every component that should exist: its kind, resolved fields, parent, and which root it hangs under (surface vs. overlay).
2. **Apply** (mutating pass over `node_map` + `Commands`) — spawn new entities, despawn removed ones, re-parent / reorder, and call the idempotent `apply_*` updaters in `render` to mirror the resolved values.

This is the retained-mode counterpart of egui's per-frame rebuild + `EditBuffers` bridge: identity is preserved by the entity map rather than re-seeded each frame.

The render loop runs as Bevy systems each frame: `collect_interactions` (widget `EntityEvent`s + text-input diffs → `PendingInteraction`) → `apply_interactions` (mutate the `MessageProcessor` via the shared core pipeline, mark the tree dirty) → `reconcile` (diff/patch the entity tree).

## Modules

| Module | Responsibility |
|------|------|
| `reconcile` | React-style diff/patch — keeps a stable `component_id → Entity` map, spawn / update / despawn / reorder so the live tree mirrors the model |
| `render` | Per-component-kind idempotent updaters — re-apply Bevy components to mirror resolved A2UI values |
| `interaction` | Maps `bevy_ui_widgets` `EntityEvent`s + text-input diffs → `PendingInteraction`, then applies via the shared core pipeline |
| `plugin` | `A2uiPlugin` — registers the render-loop systems + observers, spawns the base UI |
| `state` | `A2uiState` (`NonSend` resource) — owns the processor, function map, focus, open-modals, and the `node_map` |
| `sample_browser` | Left-hand sample list; clicking a row switches the loaded sample |

## License

MIT
