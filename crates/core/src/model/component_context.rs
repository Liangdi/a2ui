//! Component rendering context — passed to component implementations during build.

use std::collections::HashMap;

use super::components_model::SurfaceComponentsModel;
use super::data_context::DataContext;
use super::data_model::DataModel;
use crate::catalog::function_api::FunctionImplementation;

/// Transient context created for each component during rendering.
///
/// The caller is responsible for holding the RefCell borrows on DataModel
/// and SurfaceComponentsModel for the duration of rendering.
pub struct ComponentContext<'a> {
    /// The component's ID.
    pub component_id: String,
    /// The surface ID this component belongs to.
    pub surface_id: String,
    /// Scoped data access for resolving dynamic values.
    pub data_context: DataContext<'a>,
    /// The components model (escape hatch for inspecting siblings/children).
    pub components: &'a SurfaceComponentsModel,
    /// The ID of the currently focused component, if any.
    pub focused_id: Option<String>,
    /// The index of this component within a template iteration, if applicable.
    pub template_index: Option<usize>,
}

impl<'a> ComponentContext<'a> {
    /// Create a component context.
    ///
    /// Callers should borrow `surface.data_model` and `surface.components`
    /// before calling this and pass the references.
    ///
    /// The `base_path` scopes data access for this component. When it ends in a
    /// numeric segment (e.g. `/items/3` — the shape every backend produces when
    /// expanding a `ChildList::Template`), that segment is taken as the template
    /// item index and exposes the `@index` system function. Callers needing
    /// precise control can override it via [`with_template_index`](Self::with_template_index).
    pub fn new(
        component_id: String,
        surface_id: String,
        data_model: &'a DataModel,
        components: &'a SurfaceComponentsModel,
        functions: &'a HashMap<String, Box<dyn FunctionImplementation>>,
        base_path: &str,
        focused_id: Option<String>,
    ) -> Self {
        let data_context = if base_path.is_empty() {
            DataContext::new(data_model, functions)
        } else {
            DataContext::new(data_model, functions).nested(base_path.trim_start_matches('/'))
        };

        // Derive the template index from the trailing path segment so the
        // `@index` system function works without each backend having to thread
        // the index through explicitly. Template items always render at a path
        // ending in their array index (`<path>/<i>`); static components never do.
        let template_index = index_from_base_path(base_path);
        let data_context = data_context.with_template_index(template_index);

        Self {
            component_id,
            surface_id,
            data_context,
            components,
            focused_id,
            template_index,
        }
    }

    /// Override the template index (builder style), propagating it to the data
    /// context so the `@index` system function resolves correctly.
    ///
    /// `Some(i)` sets the index; `None` clears it (disabling `@index`).
    pub fn with_template_index(mut self, index: Option<usize>) -> Self {
        self.template_index = index;
        self.data_context.set_template_index(index);
        self
    }

    /// Set the template index in place, propagating it to the data context.
    pub fn set_template_index(&mut self, index: Option<usize>) {
        self.template_index = index;
        self.data_context.set_template_index(index);
    }
}

/// Derive a template item index from a (possibly relative) data path.
///
/// Returns the trailing segment parsed as a `usize` when it is a plain
/// non-negative integer (e.g. `"/items/3"` → `3`, `"items/0"` → `0`), and
/// `None` otherwise (e.g. `""`, `"/"`, `"/user"`, `"/items/3/name"`). This is
/// exactly the shape every backend emits when expanding a
/// `ChildList::Template`, so the `@index` system function is resolved without
/// per-backend plumbing.
fn index_from_base_path(path: &str) -> Option<usize> {
    path.rsplit('/').next()?.parse::<usize>().ok()
}

#[cfg(test)]
mod path_tests {
    use super::index_from_base_path;

    #[test]
    fn absolute_template_path() {
        assert_eq!(index_from_base_path("/items/0"), Some(0));
        assert_eq!(index_from_base_path("/items/42"), Some(42));
    }

    #[test]
    fn relative_template_path() {
        assert_eq!(index_from_base_path("items/7"), Some(7));
    }

    #[test]
    fn non_template_paths_yield_none() {
        assert_eq!(index_from_base_path(""), None);
        assert_eq!(index_from_base_path("/"), None);
        assert_eq!(index_from_base_path("/user"), None);
        assert_eq!(index_from_base_path("/items/3/name"), None);
    }
}
