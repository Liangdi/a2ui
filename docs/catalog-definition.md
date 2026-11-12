# Authored current catalog definitions

`CatalogDefinition` is additive and separate from the native `Catalog` registry.
`from_json(document, optional_id, optional_version)` loads released 1.0 documents;
`to_json()` preserves the authored document, except an explicit catalog ID override.
It validates Unicode XID identifiers, current activation/caller restrictions and
reference destinations, selects entries named by `anyComponent`/`anyFunction`, and
bundles pinned common types locally without fetching external resources.

`components()` and `functions()` expose selected authored schemas and metadata.
`function_args_schema(name)` supplies a standalone local bundle for runtime
argument defaults/validation, preserving common-types and authored local refs.
Schema compilation is explicitly offline; unknown external resources are denied.
Released `1.0` and earlier draft `v1.0` remain distinct.
`derive(args)` keeps named entries, accepts replacements/additions and preserves
reusable definitions/theme while dropping source-specific descriptive fields.
`validate_component(value)` validates the component and nested function calls;
explicit calls to another catalog retain generic identifier/shape checks while
that catalog's runtime owns signature checking.

`resolved_schema()` is a self-contained resource document, **not** the canonical
flattened SDK `validationSchema` representation. Use `validate_component`, not a
bare JSON Schema validation of that resource document, for recursive function
validation. No native implementation or custom UI renderer is created by loading
a document. Existing native `Catalog` structs and legacy inline parsing remain
unchanged. Current inline parsing reuses the definition validator and selection.

`validation_schema()` produces the canonical flattened current representation,
with selected unions, a component ID/type envelope, referenced common types and
authored leaf definitions. As in the reference SDK, mixin property precedence is
standard, local, then inline; this is catalog normalization, not a general-purpose
JSON Schema `allOf` equivalence transformation. Conditional constraints remain
in `allOf`. Closed nested mixins and non-object mixins are rejected explicitly
because flattening their closure/type constraints would change acceptance.
Cyclic/unknown mixin references fail, and authored document depth is capped at 128.

## Verification and remaining coverage

The unmodified pinned official `core/catalog.yaml` corpus executes 47 cases with
zero failures: current loading, published-catalog self-containment/component
validation, round-trips, derivation and both current canonical schema goldens. `catalog-coverage.json` names all 26
exclusions and the harness rejects an unexpected change in that manifest. Excluded
coverage includes legacy versions only. This is not full catalog SDK
conformance. Source schemas/fixtures are Apache-2.0; the original license is in
`crates/core/tests/conformance/LICENSE`.

## Native schema binding

`SchemaBackedCatalog` retains a current authored definition alongside an exact
native `Catalog` registry without adding fields to the existing public struct.
Construction rejects missing/extra registrations, catalog ID/name mismatches,
schema-only functions, and conflicting current return/caller/activation policies.
`to_json()` serializes the bound authored metadata; `into_catalog()` supplies the
native registrations to the existing processor. No arbitrary names are leaked.

`SchemaBackedCatalog::basic()` binds all 14 real built-in function implementations
and 18 schema-validating framework-independent component definitions to the
published basic schema. This production schema is pinned at
`a2ui-project/a2ui@eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41`,
`catalogs/basic/v1/catalog.json`, under Apache-2.0 (license in
`crates/core/src/protocol/schemas/LICENSE`). It is not loaded from test fixtures.
Tests independently compare its serialization with the published conformance
copy, exercise actual function execution/component rejection, and reject policy
mismatches. This equivalence means schemas and native registration metadata,
not proof of every visual component or all runtime function semantics. Native
renderers still own their visual implementations; existing draft builders remain
unchanged.

The binding verifies signature metadata and routing policy, not that arbitrary
native function code enforces every authored argument constraint/default or
returns semantically correct values. Those remain implementation and runtime
validation responsibilities. Loading a schema never authorizes native effects.
