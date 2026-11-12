# Official conformance fixtures

`data_model.yaml` is copied without changes from the Apache-2.0-licensed
[A2UI conformance suite](https://github.com/a2ui-project/a2ui/tree/eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41/conformance/core).
The upstream license is included in `LICENSE`.

Run `cargo test -p a2ui-base --test official_data_model -- --nocapture`.
The harness executes all 52 shared data-model cases, including every step,
error, notification and value assertion; no cases are skipped. The upstream
corpus labels these shared data-model cases v0.9; this is not evidence of
complete released-1.0 renderer conformance. Unrecognized actions/operations
fail rather than being silently ignored.

`DataModel::try_get` and `try_set` expose path errors. `subscribe_path` delivers
value-change-based exact, ancestor and descendant notifications; its handle is
disposed by dropping it. Existing `get`/`set` remain compatibility wrappers:
invalid reads return no value and invalid writes do nothing. Protocol handlers
should use fallible APIs so invalid wire messages are not silently accepted.

## Functions corpus

`functions.yaml` and the four catalog JSON fixtures are copied unmodified from
the same pinned revision. Run:

```sh
cargo test -p a2ui-base --test official_functions -- --nocapture
```

All 116 official function cases execute and pass: standalone functions,
surface-aware catalog routing, nested calls, caller restrictions, and the five
message-processing/property-evaluation scenarios. No cases are skipped.
`functions-coverage.json` pins the source and records zero known divergences.
Unknown actions and catalog paths fail. Error assertions check rejection and any
specified message fragment; cross-SDK exception category names are not asserted
because Rust exposes `A2uiError`, not the suite's `ExpressionError` classes.
The property-evaluation scenarios exercise core messages and resolved component
properties, not native widget layout or generated JSON Schema equivalence.

Released validators return `{ "valid": boolean }` when their `DataContext` is
marked with `with_current_protocol()`. Draft contexts retain boolean results.
`return_type_for_protocol` declares these version-dependent return shapes.
Hosts must propagate the surface protocol context rather than infer it from a
catalog URI. `with_catalogs` and `with_catalog_versions` enable strict per-call
catalog selection. Ordinary nested arguments use the surface default; calls
inside a `formatString` template inherit that function's selected catalog.

## Agent-function routing audit (partial)

`agent_function_routing.yaml` is the unmodified 43-case official corpus at the
same revision. Run `cargo test -p a2ui-base --test official_agent_routing -- --nocapture`.
Currently 28 cases execute and pass, with 15 explicitly named unsupported cases
in `agent-routing-coverage.json`. The harness fails any behavioral divergence,
unknown operation, or change to that omitted-case list. The remaining cases
require event/check modes, a transport-listener availability API, legacy v0.9
surface support, or non-JSON values that `serde_json::Value` cannot represent.

Executed cases compare every step's outbound calls, reactive values and errors,
including catalog selection, argument defaults, sharing, cancellation/retry,
parallel siblings, template diagnostics and imperative actions. Omitted call
`args` is compared as the protocol's empty argument map; error message assertions
use the corpus's substring convention. Partial template values and diagnostics
come from separate runtime channels, not from suppressing errors in the harness.
