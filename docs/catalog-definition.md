# Authored current catalog definitions

`CatalogDefinition` is additive and separate from the native `Catalog` registry.
`from_json(document, optional_id, optional_version)` loads released 1.0 documents;
`to_json()` preserves the authored document, except an explicit catalog ID override.
It validates Unicode XID identifiers, current activation/caller restrictions and
reference destinations, selects entries named by `anyComponent`/`anyFunction`, and
bundles pinned common types locally without fetching external resources.

`components()` and `functions()` expose selected authored schemas and metadata.
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

## Verification and remaining coverage

The unmodified pinned official `core/catalog.yaml` corpus executes 43 cases with
zero failures: current loading, published-catalog self-containment/component
validation, round-trips and derivation. `catalog-coverage.json` names all 30
exclusions and the harness rejects an unexpected change in that manifest. Excluded
coverage includes legacy versions, native SDK built-in equivalence/serialization,
and canonical flattened validation-schema goldens. This is not full catalog SDK
conformance. Source schemas/fixtures are Apache-2.0; the original license is in
`crates/core/tests/conformance/LICENSE`.
