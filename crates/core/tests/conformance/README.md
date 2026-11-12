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
