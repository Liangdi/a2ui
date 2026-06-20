pub mod basic_functions;
pub mod component_api;
pub mod function_api;
// Named `registry`, not `catalog`, to avoid `clippy::module_inception`
// (a `catalog` submodule inside the `catalog` module). The `Catalog` type it
// defines is re-exported just below, so the public path stays `catalog::Catalog`.
pub mod registry;
pub mod schema_only;

// Re-export primary types
pub use component_api::ComponentApi;
pub use function_api::FunctionImplementation;
pub use registry::Catalog;
pub use schema_only::SchemaOnlyFunction;
