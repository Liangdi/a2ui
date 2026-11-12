# A2UI GPUI Kit adapter

An opt-in adapter using **GPUI Kit 0.9.1**, the current published release checked on 2026-11-12. Kit owns the matching GPUI snapshot, component and asset versions. This is independent of the existing `a2ui-gpui` 0.2.2/0.5.1 stack; it does not force its consumers to migrate.

## Package and module boundaries

This is the standalone Cargo package `a2ui-gpui-kit`, not an extension added to the old `a2ui-gpui` package. Its own `[workspace]` and `Cargo.lock` keep GPUI Kit’s native dependency graph separate from the legacy GPUI adapter and the repository’s main workspace. The `backend` feature is opt-in; builds without it do not pull in the native GPUI runtime. Gallery/TUI dependencies are development-only, not library runtime dependencies.

The adapter PR is stacked on the current-standard core PR. It depends on that core’s schema-backed catalogs, checked ingress and resumable evaluation APIs; the adapter is not intended to compile against the older upstream core before that prerequisite lands. Package separation does not duplicate or fork the shared protocol engine.

- `app.rs`: compatibility view and native widget lifecycle; `app/validated.rs`: protected embedding facade (`ValidatedGpuiApp`).
- `walker.rs`: bounded component traversal and exact catalog dispatch; `reactive.rs`: per-instance expression/check subscriptions.
- `components.rs`: shared rendering context, layout and basic widgets; `components/choice.rs`: choice identity, selection state, rendering and tests.
- `catalog.rs`: separately identified schema-backed native subset (`native_catalog`, `NATIVE_CATALOG_ID`).
- `accessibility.rs`, `images.rs`, `theme.rs`: native semantics, image support and styling respectively.

The crate root continues to export `GpuiApp`, `ValidatedGpuiApp`, the shared interaction helpers and `customize`; widget-specific implementation modules remain internal. No extra public package or framework abstraction is needed for each widget.

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

The initial view retains the existing gallery chrome and displays one surface. Use a view per independent surface. Incoming updates reconcile widget state by surface/component/template scope. Unrelated messages retain input entities, local tabs and modal openings; authoritative value changes update existing widgets in place. Binding/range configuration changes rebuild only the affected widget. Removed scopes release entities and subscriptions. Loading/error replacements drop hidden widget focus rather than keeping an invisible editor active.

### Validated released-1.0 embedding

Prefer `ValidatedGpuiApp::new(vec![native_catalog()?], NATIVE_CATALOG_ID, cx)` for untrusted released-1.0 input. It owns a private inner view and one runtime, bootstrapped from frozen schema-backed catalogs. Send wire strings through `process_json_message`; rejected messages do not mutate the runtime. An accepted result may carry a post-commit diagnostic, which must not trigger an automatic replay. Native widget edits and incoming messages share that same runtime. `data` returns owned JSON, and no mutable processor/model/inner-entity accessor or sample loader is exposed. Transport remains host-owned; drain outgoing messages explicitly.

`GpuiApp` remains the compatibility API, including its unchecked `process_message` and trusted `processor_mut` access. Those are not strict validation boundaries. The protected facade does not accept legacy draft messages. Run the minimal protected-host example with:

```sh
cargo run --manifest-path crates/gpui-kit/Cargo.toml --features backend --example validated
```

## Negotiating the native subset

`a2ui_gpui_kit::catalog::native_catalog()` returns a `SchemaBackedCatalog` with the distinct adapter-owned ID `urn:a2ui:gpui-kit:catalog:native:1`. It derives the released basic definitions, retains the real native functions, and excludes `AudioPlayer` and `Video`. Send `to_json()` to the agent as the catalog definition, use this ID in generated surfaces, and register the returned native catalog in the runtime (or supply the schema-backed value to the core validated ingress). Its schemas reject media components rather than advertising unusable players. The canonical basic catalog remains available for compatibility, with explicit media placeholders.

This subset is a supported-widget contract, not a claim of pixel-identical rendering or complete assistive-technology parity. The metadata and presentation limits below still apply. Arbitrary custom catalog identifiers do not acquire native widget dispatch.

## Coverage and limits

- Native text inputs (including masked password fields), buttons, checkbox, slider, choices and tabs; nested rows, columns, lists, cards and modals.
- Shared core interaction/data binding and image loading. GPUI Kit's current `Separator` replaces the old `Divider` API.
- Audio/video playback is unsupported and renders an explicit unsupported-playback notice, not a functioning player. No media is downloaded or opened externally by these notices. Markdown uses the existing plain-text presentation. This adapter implements a subset of the registered basic/minimal catalogs, not complete catalog conformance: hosts must constrain generated UI to the supported components above. Registration of a canonical catalog identifies schema/function dispatch; it is not a claim that every catalog component has a native implementation. Do not advertise a modified subset under the canonical catalog identifier as a complete implementation.
- Stateful template instances use surface, component and data scope keys. Nested relative templates and button action bindings preserve their item scope. Choice labels remain separate from wire values. Repeated modal overlays retain their activation scope for content bindings, independent close actions and removal cleanup.
- Current component dispatch supports mixed registered basic/minimal catalogs using `@catalogId` then the surface default. Unknown/custom catalogs are visibly rejected, never silently rendered as a same-named basic widget. Passive functions use the surface catalog, not a merged name map.
- Current actions use the core resumable evaluator and validated argument defaults, caller/gesture policies and result types. Errors reach the host through protocol messages and diagnostic drains. Current `@checks` subscribe automatically for reachable instances: pending checks show “Checking…” without failing, failed checks show a message/border and disable buttons. Native edits recompute checks. Supported scalar properties containing `@call` (`text`, `label`, `url`, `value`, `name`, `description`, `min`, `max`, `steps`, `activeTab`) subscribe automatically with visible loading/error states; resolved values overlay rendering without changing source expressions or writable `@path` bindings. Nested option labels and tab titles also subscribe to dynamic bindings/calls without evaluating option values or child IDs. Custom component renderers and full property schemas are not yet covered. Hosts can manage additional subscriptions through `processor_mut()`.
- Subscription discovery is bounded to 512 instances/depth 64; native recursion has an independent active-path cycle/depth guard. Unsupported nodes show a diagnostic instead of recursing indefinitely. Removed template instances release their subscriptions.
- Native AccessKit metadata supports label, description, live regions and AT-hidden containers; Button/Checkbox metadata targets their real widget nodes and Input gets an explicit accessible label. Single-choice controls expose the explicit accessible name (or visible label) on the native select; multiple-choice controls use a named group of native checkboxes. Non-bound choices are disabled rather than implying edits can persist. Single-choice remains a select, not a radio group. Input textbox descriptions and select descriptions remain toolkit/API gaps: GPUI Kit 0.9.1 exposes no description builder on these widgets, and Input’s internal accessibility payload has no description field. Adding a description to an enclosing group would not fix the actual textbox. These limitations require upstream toolkit support or a separate native widget implementation, not an inaccurate role override. Metadata projection tests are not a substitute for native assistive-technology inspection.
- This branch depends on the current-protocol core update; the original GPUI adapter remains unchanged.
- The renderer is ported from `crates/gpui` under the repository's MIT license. Separate source permits Kit API evolution without changing the legacy adapter; common protocol/model code remains in `a2ui-base`.

## Verification

On Apple Silicon/macOS: library, example build, 37 unit tests and Clippy passed. Native smoke testing covered initial rendering, switching to a login form, typing into both fields, activating its button, switching away and back, and confirming form state reset. A separate current-protocol native smoke check verified the textbox accessible label, the actual button name and description, hidden decorative content absent from the accessibility tree, and the disabled-to-enabled button transition after entering a valid name. Live-region announcements and textbox descriptions were not verified. Other operating systems and complete visual/component conformance have not been tested.


The `accessibility` example exercises current reactive text, validation and native metadata without a network transport:

```sh
cargo run --manifest-path crates/gpui-kit/Cargo.toml --features backend --example accessibility
```

Initially the name field is invalid and Submit disabled. Entering a name updates the greeting, clears validation messages and enables Submit. The example's decorative text stays visually present but is hidden from assistive technology.

Pass `--stream-updates` to the accessibility example to inject unrelated model updates once a second for two minutes. Typing, selection and input focus should survive these messages; this mode does not connect to an external service.

A native streaming smoke run with the final integrated core observed the incoming-update counter advance from 11 to 18 while a middle-of-string caret was retained: typing `X` changed `Ada` to `AdXa`, and the visible greeting changed to `Hello AdXa`. The counter in the example makes ongoing updates observable. This checks input focus/caret retention, not full modal or screen-reader conformance.
