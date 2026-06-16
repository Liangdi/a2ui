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

## Component coverage

16 of the A2UI component kinds render natively (only Video / AudioPlayer stay placeholders — Bevy has no media-playback widgets, matching Iced / Slint / egui):

- **Containers / content**: Text (h1/h2/h3 heading sizes) / Row / Column / Card / List / Divider / Modal (dimmed scrim + centered panel + ✕ close button; click the scrim or ✕ to dismiss) / Button (primary blue / default gray-filled bordered / borderless transparent; click dispatches via the shared `core::components::dispatch_event`)
- **Interactive (native `bevy_ui_widgets` / `bevy_ui_text_input`, real input writes back to the data model)**: TextField (external `TextInputNode`) / CheckBox (native `Checkbox`) / Slider (native `Slider`) / ChoicePicker (the reconciler spawns a clickable row per option — single-select `●`/`○` writes `json!([value])`, multi-select `☑`/`☐` toggles array membership) / DateTimeInput (reuses the TextField `TextInputNode` bound to `value`) / Tabs (clickable tab bar + only the active panel renders; a bound `activeTab` writes back, otherwise tracked locally)
- **Icon**: mapped to an emoji (an embedded ~12 KB NotoEmoji subset font; the icon-name table matches TUI / Iced, unknown names fall back to `[first two chars]`)
- **Image**: real raster render — local paths (incl. `file://`) are read directly; `http(s)` URLs are fetched on the UI thread via `ureq` (few small sample images, same shape as Slint). Both are decoded by the `image` crate into a `bevy::image::Image` and shown via a native `ImageNode` (wgpu texture), cached by URL and cleared on sample switch; `data:` URLs and decode failures show a labeled placeholder
- **Placeholders**: Video / AudioPlayer render as labeled placeholders (no media widgets in Bevy)

### Implementation note: synthetic entities (why Tabs / ChoicePicker need reconciler special-casing)

Tabs and ChoicePicker don't use `child` / `children` (their items come from the `tabs` / `options` **properties**), and each title / option must be its **own clickable entity**. So the reconciler's `walk` special-cases them (as it does Modal): it spawns a **synthetic entity** per title / option (id prefixed `__a2ui_tab:` / `__a2ui_choice:`) carrying a `TabTitle` / `ChoiceOption` marker (with the write-back pointer). These reuse the reconciler's existing spawn / parent / orphan-cleanup machinery — switching the active tab despawns the old panel child (orphan) and spawns the new one. `bevy_ui_widgets`' `Activate` is a global trigger, so the same observer routes a synthetic-button click by marker to `TabActivate` / `ChoiceSelect` / `ChoiceToggle` instead of `ButtonActivate`.

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
| `images` | Decodes / fetches `Image` URLs into `bevy::image::Image` assets and caches the `Handle` (local read + blocking `ureq` fetch) |
| `plugin` | `A2uiPlugin` — registers the render-loop systems + observers, spawns the base UI, loads the embedded emoji icon font |
| `state` | `A2uiState` (`NonSend` resource) — owns the processor, function map, focus, open-modals, `node_map`, icon-font handle, image cache, local_tabs |
| `sample_browser` | Left-hand sample list; clicking a row switches the loaded sample |

## License

MIT
