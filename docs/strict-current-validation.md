# Opt-in current structural validation

`protocol::strict_current::StrictCurrentValidator` is a separate, stateful boundary
for released `1.0` messages. Register host-supplied `CatalogDefinition`s (and a
default), then call `validate_message` before applying a message to a renderer.
Registration never fetches network resources. Do not replace registered catalogs
while their surfaces are active. This API does not change legacy parsing or the
processor's compatibility behavior and is not automatically installed by a backend.

Validation merges incremental component updates by surface. It checks duplicate
IDs within a batch, Unicode identifiers, absolute update paths, relative binding pointer escapes, root existence,
self/circular/dangling references and unreachable components. Child references are
extracted from catalog schema declarations, including nested structured arrays and
templates, not guessed from arbitrary string-valued properties. A failed message
leaves the validator's previous state intact. Successful messages earlier in a
batch remain committed; hosts requiring transactional batches must stage them
separately.

The boundary returns `StrictValidationError` with category, message, surface and
path, plus a `renderer_error()` projection. Detailed SDK-normalized envelope and
property error paths are not yet reproduced. RPC authorization, caller policy,
transport, lifecycle ordering and native component behavior remain separate.
Mixed-catalog components are also still subject to the surface wire catalog's
schema; full cross-catalog dispatch is not claimed.

Resource bounds are explicit: 1,024 surfaces, 10,000 components per surface/batch,
50 nested levels, five nested calls, 64 schema-reference levels, and 100,000
reference/topology traversal visits. Traversal limits return errors rather than
silently omitting checks.

## Verification

Against the pinned official `validator_v1_0.yaml` corpus, this harness passes 30
complete cases and verifies three known diagnostic divergences. The three invalid
inputs are rejected as `RecursionError`; older fixtures expect `ValidationError`,
whereas newer depth-boundary fixtures explicitly expect `RecursionError`. They are
not counted as passes. The remaining 16 cases are explicitly named in
`strict-validator-coverage.json`. Five of those have independent schema-only
coverage in `official_schema`; do not add overlapping counts as separate cases.

An additional regression verifies a failed incremental update cannot corrupt the
previous valid graph. Astral-plane identifiers exercise percent-encoded generated
schema references; authored catalog documents retain their original form.
