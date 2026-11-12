# Released A2UI 1.0 compatibility

Audit baseline: [`a2ui-project/a2ui` at `eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41`](https://github.com/a2ui-project/a2ui/tree/eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41/specification/v1_0), checked 2026-11-12. `specification/versions.json` marks **1.0** current. This repository's July 2026 implementation targeted an earlier draft bearing the same name. The current protocol is **not** wire-compatible with that draft.

## Updated core paths

- Current `"1.0"` message envelopes and capabilities namespace, with the legacy `"v1.0"` namespace retained for existing examples.
- Current role-based RPC: `callRendererFunction`, `rendererFunctionResponse`, `callAgentFunction`, and `agentFunctionResponse`. Hosts drain outgoing messages for transport, and drain agent function responses for application handling.
- Remote renderer calls select an explicit registered catalog and the named surface's model. Surface-independent calls use an empty model, never an arbitrary existing surface. Functions must explicitly opt in to agent callers; user-activation functions are rejected. Return values are checked against the native declaration.
- Exactly-one result/error RPC responses, including explicit JSON null results.
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

The additive [resumable evaluator](resumable-evaluation.md) preserves the existing
synchronous API while exposing pending values explicitly. It provides shared
reactive calls, isolated imperative invocations, response resumption, cancellation,
stale-reply rejection and host-visible diagnostics. It remains opt-in; a backend
must subscribe to values and display pending/errors rather than interpreting null
as completion.

## Remaining gaps (not a full conformance claim)

- The complete official suite has not been implemented or executed. Coverage and
  named exclusions for the function-routing corpus are checked in under
  `crates/core/tests/conformance`; broader schema-generation, scaling, catalog
  derivation, RPC timeout and native-adapter suites remain outstanding.
- Automatic async property/check subscription and native UI projection require
  backend integration. The core evaluator is not proof of every renderer's
  behavior. See its prototype boundaries for transport and lifecycle details.
- Mixed-catalog custom components need host-native registrations. GPUI Kit rejects
  unsupported catalog/name pairs instead of impersonating a same-named basic
  component. Legacy backends and bundled draft samples are not all migrated.
- Inline catalog metadata extraction is not a complete catalog-definition/UAX #31
  implementation. Strict wire validation is available separately with a complete
  local catalog; runtime functions still require native implementations or an
  authorized agent transport.
- Accessibility metadata is parsed, but native screen-reader semantics, media
  decoding and platform-specific gesture behavior are not conformance-verified.

These limits remain explicit draft-PR review items.

### Caller and user-activation safety

Current and legacy remote calls enforce native caller declarations, including
nested argument calls. Legacy calls without a catalog ID require an unambiguous
function name and use an empty data model rather than an unrelated surface.
Passive evaluation and agent calls cannot acquire user activation. Only a trusted
host gesture may enter activation-capable action APIs. Schema-only calls are
validated before remote routing, and failed native calls never silently fall back
to the agent.
