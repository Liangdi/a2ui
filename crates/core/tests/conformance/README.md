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

## Functions audit (partial)

`functions.yaml` is copied unmodified from the same pinned revision. Run:

```sh
cargo test -p a2ui-base --test official_functions -- --nocapture
```

The harness accounts for all 116 cases: 77 standalone cases pass, 16 execute
but differ from the official expected behavior, and 23 require surface/catalog
resolution or renderer-validation adapters not implemented by this harness.
Every divergence and unsupported case is named in `functions-coverage.json`.
New divergences, changed observed failures, stale entries, and undocumented
omissions fail the test. A green audit test **does not** mean all function
conformance cases pass. Error assertions check rejection and any specified
message fragment; cross-SDK exception category names are not asserted because
Rust exposes `A2uiError`, not the suite's `ExpressionError` classes.

Remaining standalone divergences are regex portability: ASCII character-class
semantics, prohibited syntax/error codes, the portable static ambiguity guard,
and allowed positive lookahead. Rust's current non-backtracking regex engine
has bounded matching complexity, but that is not equivalent to implementing
the standard's dialect. Pattern and input length caps are implemented.

Released validators return `{ "valid": boolean }` when their `DataContext` is
marked with `with_current_protocol()`. Draft contexts retain boolean results.
The five dual-version native validators advertise `ReturnType::Any` because
the legacy API cannot express a version-dependent return type. Renderer hosts
must propagate the surface's protocol context, not infer it from a catalog URI.
