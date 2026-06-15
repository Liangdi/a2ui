# a2ui-slint

[![crates.io](https://img.shields.io/crates/v/a2ui-slint.svg)](https://crates.io/crates/a2ui-slint)
[![docs.rs](https://docs.rs/a2ui-slint/badge.svg)](https://docs.rs/a2ui-slint)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

English | [中文](README.md)

> 📦 Part of the **a2ui** crate ecosystem · Slint native desktop backend (optional)
>
> This crate is the second rendering backend of the [`a2ui`](https://crates.io/crates/a2ui) workspace. See the [root README](https://github.com/Liangdi/a2ui#readme) for the full introduction.

Renders an [A2UI](https://github.com/a2ui-project/a2ui) component tree onto a **native desktop window**, built on [Slint](https://slint.dev/) (pinned 1.16). The framework-agnostic interaction logic (focus traversal, event dispatch) is shared in [`a2ui-base`](https://crates.io/crates/a2ui-base), so it matches the terminal backend on keyboard / button behavior.

> **Optional and heavy**: this crate is a **non-default workspace member** — a plain `cargo build` does not compile it.

## Where it fits

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery  │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui            │
│  a2ui-base  (framework-agnostic: Protocol / Model / Catalog / Processor) │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-slint` depends on [`a2ui-base`](https://crates.io/crates/a2ui-base); it is consumed by `a2ui-slint-gallery` and (under the `slint` feature) the umbrella `a2ui`.

## Building

Everything lives behind the `backend` cargo feature, which is what pulls in the Slint runtime. Without that feature the crate is an empty shell (no dependencies beyond `a2ui-base`), keeping the workspace's default build light.

```bash
cargo build -p a2ui-slint --features backend
```

The renderer uses `renderer-software` + `backend-winit`, so it runs **without a GPU / OpenGL driver**.

## Component coverage

All 18 A2UI component types render:

- **Rich**: Text / Button / Column / Row / Card / TextField / CheckBox / Slider (Button and CheckBox clicks dispatch through the shared `core::components::dispatch_event`)
- **Best-effort**: Divider / Icon / Tabs / Modal / List / ChoicePicker / DateTimeInput
- **Placeholder**: Image / Video / AudioPlayer render as labeled placeholders

## Implementation note: why the tree is flattened

Slint **cannot express recursion** (neither recursive structs nor self-referential components — see [slint-ui/slint#4218](https://github.com/slint-ui/slint/issues/4218)). So `live_tree` is not a nested tree; it flattens the component tree into a `Vec<LiveNode>` with index-based `children` references, and `build.rs` code-generates a **bounded-depth** component chain `Node0` (leaf) → … → `Node7` (root). A2UI trees are usually shallow, so depth 7 covers real UIs; deeper subtrees are truncated to `…`.

> This is the key constraint future contributors need to know. If you need native recursion + truly editable input controls, use the [`a2ui-egui`](https://crates.io/crates/a2ui-egui) backend instead.

## Modules

| Module | Responsibility |
|------|------|
| `live_tree` | The flattened node array (works around Slint's recursion limit) |
| `host` | `SurfaceHost::run` — owns state, drives the Slint event loop |
| `ui` | The generated module via `include_modules!()` + the `LiveNode` type |

## License

MIT
