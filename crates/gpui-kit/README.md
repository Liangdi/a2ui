# A2UI GPUI Kit adapter

An opt-in adapter using **GPUI Kit 0.9.1**, the current published release checked on 2026-11-12. Kit owns the matching GPUI snapshot, component and asset versions. This is independent of the existing `a2ui-gpui` 0.2.2/0.5.1 stack; it does not force its consumers to migrate.

## Build and run

Rust 1.99 or later is required by the current GPUI dependency line. The backend is disabled by default and has its own lockfile/workspace so the two GPUI stacks do not unify incompatible dependencies.

```sh
cargo check --manifest-path crates/gpui-kit/Cargo.toml
cargo test --manifest-path crates/gpui-kit/Cargo.toml --features backend --lib
cargo run --manifest-path crates/gpui-kit/Cargo.toml --features backend --example gallery
```

The gallery reuses the existing TUI catalogs and embedded samples. Its TUI dependency needs `chafa` development files (`brew install chafa` on macOS). The renderer library itself does not depend on the TUI. On macOS, launch GUI examples from a logged-in graphical session; sandboxed shell sessions may lack pasteboard/window-server access.

## Embedding

Enable the `backend` feature, initialize with `gpui_kit::init`, construct `GpuiApp::new(catalogs, functions)`, and mount it in a Kit `Root` (or use `gpui_kit::open_window`). No separate GPUI or asset dependency is needed.

Apply incoming messages with `GpuiApp::process_message` inside an entity update and call `cx.notify()` after success. Drain `drain_outgoing_messages()` to send user events back through your own transport. The adapter does not open network connections for agent traffic or execute generated code.

The initial view retains the existing gallery chrome and displays one surface. Use a view per independent surface. Incoming updates invalidate local widget state so native inputs are seeded from the authoritative data model; focus-preserving incremental reconciliation is a future improvement.

## Coverage and limits

- Native text inputs, buttons, checkbox, slider, choices and tabs; nested rows, columns, lists, cards and modals.
- Shared core interaction/data binding and image loading. GPUI Kit's current `Separator` replaces the old `Divider` API.
- Video and audio remain placeholders, and Markdown uses the existing plain-text presentation.
- Protocol conformance follows the shared core. This adapter does not claim to complete released A2UI 1.0 mixed-catalog rendering, automatic agent-function actions, or all accessibility semantics.
- The renderer is ported from `crates/gpui` under the repository's MIT license. Separate source permits Kit API evolution without changing the legacy adapter; common protocol/model code remains in `a2ui-base`.

## Verification

On Apple Silicon/macOS: library, example build, 12 unit tests and Clippy passed. Native smoke testing covered initial rendering, switching to a login form, typing into both fields, activating its button, switching away and back, and confirming form state reset. Other operating systems and complete visual/component conformance have not been tested.
