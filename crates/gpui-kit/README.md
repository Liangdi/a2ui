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

Apply incoming messages with `GpuiApp::process_message` inside an entity update and call `cx.notify()` after success. Drain `drain_outgoing_messages()` through your own transport. Draft surfaces retain legacy `v1.0` button events. Current `1.0` surfaces emit current events (including resolved `userMessage`) and route function actions through the exact selected catalog: native functions run locally; declared schema-only and undeclared non-system functions emit `callAgentFunction` after policy checks. Nested asynchronous event context and userMessage values resolve before the final event is emitted. Each imperative call gets a fresh ID. Drain `drain_agent_function_responses()` to handle returned values/errors in your host. Use `processor_mut()` to start/cancel opt-in expression or check subscriptions and drain their updates/diagnostics. The adapter does not open network connections for agent traffic or execute generated code.

The initial view retains the existing gallery chrome and displays one surface. Use a view per independent surface. Incoming updates invalidate local widget state so native inputs are seeded from the authoritative data model; focus-preserving incremental reconciliation is a future improvement.

## Coverage and limits

- Native text inputs (including masked password fields), buttons, checkbox, slider, choices and tabs; nested rows, columns, lists, cards and modals.
- Shared core interaction/data binding and image loading. GPUI Kit's current `Separator` replaces the old `Divider` API.
- Video and audio remain placeholders, and Markdown uses the existing plain-text presentation.
- Stateful template instances use surface, component and data scope keys. Nested relative templates and button action bindings preserve their item scope. Choice labels remain separate from wire values. Template-repeated modal overlays still need scoped lifecycle work.
- Current component dispatch supports mixed registered basic/minimal catalogs using `@catalogId` then the surface default. Unknown/custom catalogs are visibly rejected, never silently rendered as a same-named basic widget. Passive functions use the surface catalog, not a merged name map.
- Current actions use the core resumable evaluator and validated argument defaults, caller/gesture policies and result types. Errors reach the host through protocol messages and diagnostic drains. Current `@checks` subscribe automatically for reachable instances: pending checks show “Checking…” without failing, failed checks show a message/border and disable buttons. Native edits recompute checks. Supported scalar properties containing `@call` (`text`, `label`, `url`, `value`, `name`, `description`, `min`, `max`, `steps`, `activeTab`) subscribe automatically with visible loading/error states; resolved values overlay rendering without changing source expressions or writable `@path` bindings. Nested option/tab-label objects, custom component renderers and full property schemas are not yet covered. Hosts can manage additional subscriptions through `processor_mut()`.
- Subscription discovery is bounded to 512 instances/depth 64; native recursion has an independent active-path cycle/depth guard. Unsupported nodes show a diagnostic instead of recursing indefinitely. Removed template instances release their subscriptions.
- Native AccessKit metadata supports label, description, live regions and AT-hidden containers; Button/Checkbox metadata targets their real widget nodes and Input gets an explicit accessible label. Input textbox descriptions and ChoicePicker radio-group semantics remain toolkit/API gaps. Metadata projection tests are not a substitute for native assistive-technology inspection.
- This branch depends on the current-protocol core update; the original GPUI adapter remains unchanged.
- The renderer is ported from `crates/gpui` under the repository's MIT license. Separate source permits Kit API evolution without changing the legacy adapter; common protocol/model code remains in `a2ui-base`.

## Verification

On Apple Silicon/macOS: library, example build, 31 unit tests and Clippy passed. Native smoke testing covered initial rendering, switching to a login form, typing into both fields, activating its button, switching away and back, and confirming form state reset. A separate current-protocol native smoke check verified the textbox accessible label, the actual button name and description, hidden decorative content absent from the accessibility tree, and the disabled-to-enabled button transition after entering a valid name. Live-region announcements and textbox descriptions were not verified. Other operating systems and complete visual/component conformance have not been tested.


The `accessibility` example exercises current reactive text, validation and native metadata without a network transport:

```sh
cargo run --manifest-path crates/gpui-kit/Cargo.toml --features backend --example accessibility
```

Initially the name field is invalid and Submit disabled. Entering a name updates the greeting, clears validation messages and enables Submit. The example's decorative text stays visually present but is hidden from assistive technology.
