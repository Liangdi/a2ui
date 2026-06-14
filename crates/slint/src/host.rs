//! Runtime host — owns the Slint window, the message processor, and the bridge
//! between UI events and the framework-agnostic interaction layer in `a2ui_core`.
//!
//! [`SurfaceHost`] is the Slint counterpart of the tui gallery's `GalleryApp`:
//! it holds the [`MessageProcessor`] state, renders the first surface into the
//! `Surface` window via [`live_tree`], and routes node activations (button
//! presses) through [`a2ui_core::components::dispatch_event`] +
//! [`a2ui_core::interaction::apply_event_result`].
//!
//! Shared state lives behind `Rc` so the Slint `Events.activate` callback (set
//! once at construction) can reach back into the processor. Slint runs on a
//! single thread, so `Rc`/`RefCell` (not `Arc`/`Mutex`) are sufficient.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use slint::ComponentHandle;

use a2ui_core::catalog::function_api::FunctionImplementation;
use a2ui_core::components::dispatch_event;
use a2ui_core::event::{InputEvent, InputKey};
use a2ui_core::focus::FocusManager;
use a2ui_core::interaction::apply_event_result;
use a2ui_core::message_processor::MessageProcessor;
use a2ui_core::model::component_context::ComponentContext;
use a2ui_core::protocol::server_to_client::A2uiMessage;

use crate::live_tree::build_nodes;
use crate::ui::{Events, LiveNode, Surface};

/// Owns a Slint window bound to a single A2UI surface.
pub struct SurfaceHost {
    state: Rc<HostState>,
}

/// The shared, interior-mutable state behind the host.
struct HostState {
    surface: Surface,
    processor: RefCell<MessageProcessor>,
    /// All catalog functions merged into one map (function names are globally
    /// unique), used to resolve dynamic values while walking + dispatching.
    functions: HashMap<String, Box<dyn FunctionImplementation>>,
    focus: RefCell<FocusManager>,
}

impl SurfaceHost {
    /// Create a host: register `catalogs` with a fresh processor, create the
    /// window, wire the activation callback, and render the initial frame.
    ///
    /// `functions` is the merged function map (the same implementations the
    /// `catalogs` carry — passed separately because [`MessageProcessor`] owns
    /// the catalogs and doesn't expose their functions).
    pub fn new(
        catalogs: Vec<a2ui_core::catalog::Catalog>,
        functions: HashMap<String, Box<dyn FunctionImplementation>>,
    ) -> Result<Self, slint::PlatformError> {
        let processor = MessageProcessor::new(catalogs);
        let focus = FocusManager::new();
        let surface = Surface::new()?;

        let state = Rc::new(HostState {
            surface,
            processor: RefCell::new(processor),
            functions,
            focus: RefCell::new(focus),
        });

        // Route button presses → core dispatch.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_activate(move |node_id| s.handle_activate(node_id.as_str()));
        }

        state.redraw();
        Ok(SurfaceHost { state })
    }

    /// Feed an A2UI message (createSurface / updateComponents / updateDataModel / …).
    pub fn process_message(&self, message: A2uiMessage) {
        let _ = self.state.processor.borrow_mut().process_message(message);
        self.rebuild_focus();
        self.state.redraw();
    }

    /// Rebuild the focus list from the current component tree.
    pub fn rebuild_focus(&self) {
        let proc = self.state.processor.borrow();
        if let Some(surface) = proc.model.surfaces().next() {
            let components = surface.components.borrow();
            self.state.focus.borrow_mut().rebuild_from_components(&components);
        }
    }

    /// Cycle focus forward / backward (Tab / Shift-Tab) and redraw.
    pub fn focus_next(&self) {
        self.state.focus.borrow_mut().focus_next();
        self.state.redraw();
    }
    pub fn focus_prev(&self) {
        self.state.focus.borrow_mut().focus_prev();
        self.state.redraw();
    }

    /// Show the window and run the Slint event loop until it closes.
    pub fn run(&self) -> Result<(), slint::PlatformError> {
        self.state.surface.run()
    }
}

impl HostState {
    /// Re-walk the surface and push a fresh node array into the window.
    fn redraw(&self) {
        let proc = self.processor.borrow();
        let Some(surface) = proc.model.surfaces().next() else {
            return;
        };
        let focused = self.focus.borrow().focused_id().map(str::to_string);
        let nodes = build_nodes(surface, &self.functions, focused.as_deref());
        self.surface.set_nodes(to_node_model(nodes));
    }

    /// A node was activated (button press): dispatch Enter to its `handle_event`
    /// via the shared core logic, apply the result, and redraw.
    fn handle_activate(&self, node_id: &str) {
        // Resolve the component type + build a context (shared borrow, then dropped).
        let result = {
            let proc = self.processor.borrow();
            let Some(surface) = proc.model.surfaces().next() else {
                return;
            };
            let comp_type = match surface.components.borrow().get(node_id) {
                Some(m) => m.component_type.clone(),
                None => return,
            };
            let data_model = surface.data_model.borrow();
            let components = surface.components.borrow();
            let ctx = ComponentContext::new(
                node_id.to_string(),
                surface.id.clone(),
                &data_model,
                &components,
                &self.functions,
                "",
                Some(node_id.to_string()),
            );
            dispatch_event(
                &comp_type,
                &ctx,
                &InputEvent::KeyPress { key: InputKey::Enter },
            )
        };

        if let Some(result) = result {
            let mut proc = self.processor.borrow_mut();
            let _ = apply_event_result(&mut proc, result);
        }
        self.redraw();
    }
}

/// Wrap a `Vec<LiveNode>` into the `ModelRc` shape the `nodes` property expects.
fn to_node_model(nodes: Vec<LiveNode>) -> slint::ModelRc<LiveNode> {
    slint::ModelRc::new(Rc::new(slint::VecModel::from(nodes)))
}
