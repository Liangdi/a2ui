# Released A2UI 1.0 compatibility

Audit baseline: [`a2ui-project/a2ui` at `eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41`](https://github.com/a2ui-project/a2ui/tree/eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41/specification/v1_0), checked 2026-11-12. `specification/versions.json` marks **1.0** current. This repository's July 2026 implementation targeted an earlier draft bearing the same name. The current protocol is **not** wire-compatible with that draft.

## Updated core paths

- Current `"1.0"` message envelopes and capabilities namespace, with the legacy `"v1.0"` namespace retained for existing examples.
- Current role-based RPC: `callRendererFunction`, `rendererFunctionResponse`, `callAgentFunction`, and `agentFunctionResponse`. Hosts drain outgoing messages for transport, and drain agent function responses for application handling.
- Remote renderer calls select an explicit registered catalog and the named surface's model. Surface-independent calls use an empty model, never an arbitrary existing surface. The top-level target must explicitly opt in to agent callers. Current nested arguments are renderer-local expressions: renderer-only native functions are allowed, agent-only functions are rejected, and no remote fallback or cached agent result is used. Ordinary wire processing has no user activation; a trusted host may explicitly invoke the separate gesture entry point. Return values are checked against the native declaration.
- Exactly-one result/error RPC responses, including explicit JSON null results.
- Host-driven outgoing RPC lifecycle: optional deadlines, deterministic expiry, surface cancellation, disposal, duplicate/late-reply rejection and explicit transport availability. Hosts advance deadlines and drain completion queues; this does not add network retries or remote cancellation.
- `@path` and `@call` dynamic directives; recursive arrays, literal objects and null; literal `path`/`call` dictionaries; doubled `@@` literal-key escaping. Fallible evaluation reports malformed directives and unavailable catalog overrides.
- Optional default surface catalog and preserved surface metadata; `@catalogId`, `@metadata`, `@checks` and `@accessibility` component accessors.
- Required `updateDataModel.value` on current messages, preserving explicit null deletion through deserialization and serialization.
- Current messages reject multiple envelope payloads, reserved `Surface` components, empty component batches and removed surface styling properties.

## Migration and compatibility

Use `"version": "1.0"` in new payloads and the `"1.0"` capability key. Within dynamic values use `{"@path":"/name"}` and `{"@call":"formatString","args":{...}}`. **Do not** rename the `path` parameter in `updateDataModel` or child templates. Typed scalar properties accept old directive aliases for existing consumers, but general dynamic values use current semantics: ordinary `path` and `call` keys are data, not instructions. Nested old-style function arguments must be migrated deliberately.

`FunctionCall` adds an optional `catalog_id` Rust field. Existing Rust literals should set `catalog_id: None`. Existing native functions default to `AllowedCallers::RendererOnly`; implementations intending remote invocation must override `allowed_callers`. This protects existing local functions from unintended remote invocation.

Legacy `callFunction`/`actionResponse` and helpers remain available only as draft APIs. They are not emitted by the new role-based RPC path. Use explicit `call_agent_function` for current remote invocation; agent responses are exposed rather than silently written into legacy action-response state.

## Verified coverage and optional strict validation

The pinned official `functions.yaml` corpus now passes all **116 cases** with no
skips. The shared `data_model.yaml` corpus passes all **52 cases** with no skips
(the source labels these shared semantics v0.9). The portable regex implementation
also has 160 checked-in reference vectors and was compared with the upstream
Python reference across 9,810 cases without a mismatch. These are exact suite
claims, not a claim that every upstream conformance suite has run.

Additional pinned corpus results, with named exclusions checked in under
`crates/core/tests/conformance`:

- Function routing: **41/43 pass**, with two exclusions (legacy v0.9 and non-JSON values).
- Catalog definitions: **43/73 pass**, with 30 exclusions covering legacy APIs,
  native SDK built-in equivalence/serialization and canonical flattened schemas.
- RPC functions: **28/33 pass**, with five UI binding/check/action exclusions.
  Passing cases cover incoming calls, caller/activation boundaries, reply
  correlation, timeout, cancellation, disposal and late replies. The excluded
  cases require the official mounted resolver semantics, structured rule results,
  action completion values or correlated surface errors not exposed by this host API.

`CurrentSchemaValidator::new(catalog_json)` compiles the pinned released incoming
and outgoing schemas against a locally supplied negotiated catalog. Use
`parse_incoming` to validate the original wire value before typed parsing.
External HTTP/file schema loading is disabled. This additive strict boundary does
not change the legacy parser's compatibility behavior and does not substitute
for state-machine/topology validation.

Current native validators return structured `ValidationResult` objects; draft
contexts keep boolean results. Passive function evaluation can use an explicit
catalog registry and protocol-version map. Each nested call follows its own
surface default/explicit override, while template parts stay in the selected
function's catalog. Schema-only functions retain argument schemas, defaults,
caller restrictions, activation requirements and declared return types.

The additive [CatalogDefinition](catalog-definition.md) loads and validates
authored current catalogs, preserves authored round-trips and supports derivation,
Unicode XID identifiers, local references and offline common-type bundling. It is
separate from the native `Catalog` registry: loading schemas does not create native
functions or custom component renderers. Its self-contained resource document is
not the SDK's canonical flattened validation schema.

The additive [resumable evaluator](resumable-evaluation.md) preserves the existing
synchronous API while exposing pending values explicitly. It provides shared
reactive calls, isolated imperative invocations, response resumption, cancellation,
stale-reply rejection and host-visible diagnostics. It remains opt-in; a backend
must subscribe to values and display pending/errors rather than interpreting null
as completion.

## Remaining gaps (not a full conformance claim)

- The complete official suite has not been implemented or executed. The exact
  catalog, routing and RPC exclusions above remain open; broader schema-generation,
  scaling and native-adapter conformance suites remain outstanding.
- Backend integration is separate from core evaluation. The dependent GPUI Kit
  adapter now subscribes to supported scalar properties and checks automatically,
  with loading/error states and disabled buttons on failed checks. Nested dynamic
  option/tab-label structures and full property schemas remain incomplete. These
  backend tests do not satisfy the five excluded official mounted-UI RPC cases.
- Mixed-catalog custom components need host-native registrations. GPUI Kit rejects
  unsupported catalog/name pairs instead of impersonating a same-named basic
  component. Legacy backends and bundled draft samples are not all migrated.
- Catalog definition coverage does not establish full native SDK equivalence,
  legacy catalog compatibility or canonical flattened-schema generation. Runtime
  functions still require native implementations or an authorized agent transport.
- The dependent GPUI Kit native smoke test verified a textbox label, button name
  and description, and Submit changing from disabled to enabled after entering
  `Ada`. Full assistive-technology semantics remain unverified; input descriptions,
  choice-group semantics, media playback and platform-specific gesture behavior
  remain gaps. Other backends do not inherit these native verification results.

These limits remain explicit draft-PR review items.

### Caller and user-activation safety

Current remote calls enforce agent authorization on the top-level target and renderer authorization on nested expressions. Legacy calls retain the conservative agent-caller check for nested argument calls. Legacy calls without a catalog ID require an unambiguous
function name and use an empty data model rather than an unrelated surface.
Passive evaluation and agent calls cannot acquire user activation. Only a trusted
host gesture may enter activation-capable action APIs. Schema-only calls are
validated before remote routing, and failed native calls never silently fall back
to the agent.


### Incoming RPC caller scopes

The current caller distinction follows the pinned official Python RPC handler
(`rpc_handler.py`, `_check_execution_permissions` and `_resolve_dynamic_arguments`),
`DataContext.resolve_without_agent`, and the TypeScript handler's
`resolveArgumentValue(..., {allowAgentRouting: false})` at revision
`eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41`. Official native builtins are not
relabelled as agent-callable. Nested expressions still enforce argument validation,
activation requirements, catalog/version boundaries, and native availability.

`process_json_message` additionally emits a correlated protocol error for malformed
current renderer RPC envelopes when a string function-call ID can be recovered.
Execution-stage failures (including nested expression failures) return
`EXECUTION_ERROR`; invalid top-level target authorization/schema returns
`INVALID_FUNCTION_CALL`.
