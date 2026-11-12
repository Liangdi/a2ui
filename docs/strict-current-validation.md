# Opt-in checked current ingress

`protocol::validated_processor::ValidatedMessageProcessor::new` accepts exact
`SchemaBackedCatalog` registrations and an explicit default catalog. It constructs
an empty current-only runtime and freezes its registry. Incoming JSON and typed
messages are checked before they can mutate the processor. Owned component/data
snapshots, interaction forwarding and outgoing-message drains expose the runtime
without raw model/processor mutation handles. Existing `MessageProcessor` behavior
and draft compatibility remain unchanged.

A renderer that must own its processor can instead use
`CheckedIngress::bootstrap(catalogs, default_catalog)`, which returns a checked
ingress guard and its empty, matching processor. Keep that pair private and route
all agent messages through the guard. It resynchronizes structural state from the
actual processor before each checked message, including trusted local changes.
It does **not** turn other public `MessageProcessor` mutation APIs into checked
APIs or defend against arbitrary trusted Rust code replacing native registrations.
The lower-level `CheckedIngress::new` requires an empty processor with matching
catalog IDs; hosts using it must establish schema/native equivalence themselves.
Prefer `bootstrap` for exact `SchemaBackedCatalog` pairing.

Both entry points prepare a single-surface validation change, call the processor
once, and commit validator state only when the processor accepts the message.
Pre-commit errors leave prior structural state intact. `IngressAccepted` can carry
a `post_commit_diagnostic` from subscription refresh: the message was accepted,
so this diagnostic must not be presented as rollback or trigger blind replay.
There is no second processor and no duplicate native/evaluator execution.

## Structural checks

The underlying `StrictCurrentValidator` validates the transport envelope separately
from each component's negotiated `@catalogId`. Calls nested in component properties
use the **surface** function catalog unless they have an explicit `catalogId`,
including calls nested inside another catalog's function arguments. Unknown
catalogs, undeclared functions, invalid arguments and agent-only calls in renderer
bindings/actions fail closed. Native activation/caller policy is additionally
checked by the processor at execution time.

Incremental component updates merge by surface. Checks cover duplicate IDs within
a batch, Unicode identifiers, absolute update paths, relative binding pointer
escapes, root existence, self/circular/dangling references and unreachable nodes.
Composition checks enforce each actual parent's `allowedChildren` and each
child's `allowedParents`, with the real root checked under `Surface`. Policies
from local references and `allOf` are supported; conditional composition metadata
(`anyOf`/`oneOf`/`if`/`then`/`else`/`not`) fails closed rather than being ignored.
Composition diagnostics retain `UNALLOWED_PARENT`/`UNALLOWED_CHILD` codes, but paths
use component IDs rather than official batch indices.

Child references come from schema declarations, including structured arrays and
templates, rather than arbitrary string-valued properties. Registration through
the standalone validator is rejected while surfaces are active.

Limits are explicit: 1,024 surfaces, 10,000 components per surface/batch, 100,000
components across surfaces, 50 nested levels, five nested calls, 64 reference-schema
levels, and 100,000 reference/topology traversal visits. Limits fail explicitly.
The envelope schema is compiled once; staged validation does not clone all catalog
definitions. Renderer snapshot resynchronization copies structural components but
not catalog schemas or data-model contents.

## Verification and remaining scope

The pinned official `validator_v1_0.yaml` harness passes 33 complete cases with no
known divergence. Category checks mirror the Python reference's exception
inheritance: `RecursionError` also satisfies `ValidationError`, rather than being
incorrectly counted as a differing result. Sixteen cases remain named exclusions;
five have separate schema-only coverage in `official_schema` (38 unique cases).
Detailed SDK-normalized envelope/property paths and diagnostic arrays remain
incomplete; this is not a full validator-conformance claim.

Additional regressions cover mixed component/function catalogs, nested function
overrides, rejected catalog replacement, failed incremental updates, runtime
rejection without validator drift, owned snapshot isolation and resynchronization
after trusted host graph changes. Native components outside a renderer's supported
subset are not made renderable by this core wrapper. Hosts must negotiate a truthful
renderer subset. The existing legacy backends are not automatically opted in.

## Error wire projection

`StrictValidationError.path` retains the local diagnostic location, which can be
an internal component-graph path. `renderer_error()` returns `Option<Value>` and
uses the honest root pointer for validation errors with a known surface.
`renderer_error_for_message(&original_message)` can recover a surface or RPC call
ID from one unambiguous recognized payload and retain a path only if it resolves
in that original payload. Graph-ID paths are never presented as array indices.

The official outgoing schema requires correlation even for generic errors. `None`
therefore means a local/transport-level diagnostic, **not** an outgoing wire event;
no surface or call ID is fabricated. Generic errors omit `path`, and reserved
validation codes without a surface become the generic `INVALID_MESSAGE` code when
a real RPC call ID is available. Regression tests validate emitted shapes against
the pinned official outgoing schema for every supported error-code category.
