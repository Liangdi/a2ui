# Message processor scaling audit

Source: official `conformance/core/message_processor_scaling.yaml`, pinned to
`eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41`. The unmodified YAML and composition
catalog are vendored under `crates/core/tests/conformance` (Apache-2.0).

Run apart from other tests, without concurrent builds or benchmarks:

```sh
cargo test -p a2ui-base --test official_message_processor_scaling -- --ignored --test-threads=1 --nocapture
```

The harness expands the actual official payload templates at 200/800 components
(basic relaxed and no validation) and 1000/4000 (composition relaxed). Each size
warms up once. Five alternating samples per size produce median durations and
an enforced growth ratio at most 8. Unknown action, operation, validation setting
or catalog path fails. Every message result and the resulting component count
and final item are checked. Payload expansion and decoding occur before timing;
processor creation, inline catalog registration and one `process_messages` call
are inside timing. Fixed catalog registration cost is therefore included in
both sizes and should be considered when interpreting the ratio.

## Scope and gaps

These are real processor timing measurements, not proof of all official
validation semantics. Current `with_validation(RELAXED)` checks incoming batch
integrity/topology, not the entire catalog's JSON Schema. In particular it does
not enforce `allowedParents` / `allowedChildren` composition constraints. The
third case is explicitly labeled **measured only**, even if its growth ratio
meets the bound. Do not count it as composition conformance. Native rendering,
stream parsing and catalog-schema validation overhead are not measured here.

The test is intentionally ignored in ordinary unit test runs because the
upstream suite requires isolated timing. Running it without `--ignored` does
not establish a passing scaling result.

A second, nested timing interval measures just `process_messages`; its independent
medians must also meet the same growth bound. This prevents fixed catalog setup
cost from hiding a processing regression. In the initial isolated cloud debug
run, processing-only 4n/n ratios were 3.865 (basic relaxed), 3.911 (basic none),
and 4.132 (composition measured-only). These are one-machine observations, not
latency guarantees. No composition semantics were added by this audit.
