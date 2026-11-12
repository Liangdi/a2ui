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

## Remaining gaps (not a full conformance claim)

- Existing renderers still use their own component registries. Mixed-catalog component rendering and automatic agent-function actions require renderer-level work, not just parsing `@catalogId`.
- Existing bundled catalog schemas, sample payloads and built-in validation function result shapes have not all been migrated to the released basic catalog. Strict schema-validation and complete official conformance-suite execution remain outstanding.
- Inline catalog loading is a lightweight schema parser, not a full JSON Schema/UAX #31 validator. Runtime functions require native implementations; inline declarations do not grant executable behavior.
- Accessibility metadata is parsed, but each backend must implement native screen-reader semantics. Media decoding, gesture-gated functions and platform-specific behavior are not validated by the core tests.

These limits are deliberate review items for the draft PR, not hidden compatibility guarantees.
