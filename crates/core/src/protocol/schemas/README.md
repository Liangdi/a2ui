# Released A2UI 1.0 schemas

Unmodified Apache-2.0 schema sources from a2ui-project/a2ui at
`eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41`,
`specification/v1_0/json/`. License included alongside the files.

`CurrentSchemaValidator::new(catalog_json)` builds incoming/outgoing validators
using the actual negotiated catalog and locally registered common types. Remote
HTTP and file reference fetching are disabled. Unknown external references fail
construction. This is an optional, additive strict boundary; existing parser
entry points keep their compatibility behavior. Prefer `parse_incoming` to
validate the original JSON before typed parsing.

Schema validity alone does not prove message ordering, topology, mixed-catalog
component resolution, renderer behavior, or complete protocol conformance.
