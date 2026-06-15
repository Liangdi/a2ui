//! Runtime host — owns the Slint window, the message processor, and the bridge
//! between UI events and the framework-agnostic interaction layer in `a2ui_base`.
//!
//! [`SurfaceHost`] is the Slint counterpart of the tui gallery's `GalleryApp`:
//! it holds the [`MessageProcessor`] state, renders the first surface into the
//! `Surface` window via [`live_tree`], and routes node activations (button
//! presses) through [`a2ui_base::components::dispatch_event`] +
//! [`a2ui_base::interaction::apply_event_result`].
//!
//! Shared state lives behind `Rc` so the Slint `Events.activate` callback (set
//! once at construction) can reach back into the processor. Slint runs on a
//! single thread, so `Rc`/`RefCell` (not `Arc`/`Mutex`) are sufficient.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use slint::ComponentHandle;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::components::dispatch_event;
use a2ui_base::event::{InputEvent, InputKey};
use a2ui_base::focus::FocusManager;
use a2ui_base::interaction::apply_event_result;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::server_to_client::A2uiMessage;

use crate::live_tree::{build_nodes, build_overlay_nodes};
use crate::ui::{Events, LiveNode, SampleEntry, Surface};

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
    /// Gallery samples: `(name, messages)`. Selection replays a sample's messages.
    samples: RefCell<Vec<(String, Vec<A2uiMessage>)>>,
    /// Locally-tracked open Modal component ids. The gallery has no server to
    /// flip a Modal's `isOpen`, so trigger activations are recorded here and
    /// surfaced to `build_nodes` as the open state. Cleared on sample switch.
    open_modals: RefCell<HashSet<String>>,
}

impl SurfaceHost {
    /// Create a host: register `catalogs` with a fresh processor, create the
    /// window, wire the activation callback, and render the initial frame.
    ///
    /// `functions` is the merged function map (the same implementations the
    /// `catalogs` carry — passed separately because [`MessageProcessor`] owns
    /// the catalogs and doesn't expose their functions).
    pub fn new(
        catalogs: Vec<a2ui_base::catalog::Catalog>,
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
            samples: RefCell::new(Vec::new()),
            open_modals: RefCell::new(HashSet::new()),
        });

        // Route button presses → core dispatch.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_activate(move |node_id| s.handle_activate(node_id.as_str()));
        }
        // Route gallery sidebar clicks → load the selected sample.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .on_select_sample(move |idx| s.select(idx as usize));
        }
        // Clicking the modal overlay's backdrop closes any open modal.
        {
            let s = Rc::clone(&state);
            state.surface.on_close_modal(move || {
                s.open_modals.borrow_mut().clear();
                s.redraw();
            });
        }

        state.redraw();
        Ok(SurfaceHost { state })
    }

    /// Populate the left-hand sample browser with `samples` `(name, messages)`
    /// pairs, then load the sample at `initial` into the right-hand pane.
    pub fn set_samples(&self, samples: Vec<(String, Vec<A2uiMessage>)>, initial: usize) {
        let entries: Vec<SampleEntry> = samples
            .iter()
            .map(|(name, _)| SampleEntry { name: name.as_str().into() })
            .collect();
        let model = slint::ModelRc::new(Rc::new(slint::VecModel::from(entries)));
        self.state.surface.set_samples(model);
        self.state.samples.borrow_mut().clear();
        self.state.samples.borrow_mut().extend(samples);
        self.state.select(initial);
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
        let open_modals = self.open_modals.borrow();
        let nodes = build_nodes(surface, &self.functions, focused.as_deref(), &open_modals);
        self.surface.set_nodes(to_node_model(nodes));
        let overlay_nodes =
            build_overlay_nodes(surface, &self.functions, focused.as_deref(), &open_modals);
        self.surface.set_overlay_visible(!overlay_nodes.is_empty());
        self.surface.set_overlay_nodes(to_node_model(overlay_nodes));
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

        // Modal open/close is handled locally (no server): activating a Modal's
        // trigger opens it; activating a Modal node itself (the open panel is
        // click-to-close) toggles it shut.
        self.apply_modal_interaction(node_id);
        self.redraw();
    }

    /// Resolve a node activation into a local Modal state change, if any.
    ///
    /// Two cases, derived from the catalog's trigger/content semantics rather
    /// than the (server-routed) event name: activating a component that is some
    /// Modal's `trigger` opens that Modal; activating a Modal node directly
    /// (e.g. clicking its open panel) toggles it closed.
    fn apply_modal_interaction(&self, node_id: &str) {
        let proc = self.processor.borrow();
        let Some(surface) = proc.model.surfaces().next() else {
            return;
        };
        let components = surface.components.borrow();

        let is_modal = components
            .get(node_id)
            .map(|m| m.component_type == "Modal")
            .unwrap_or(false);

        // If this node is some Modal's trigger, that Modal should open.
        let opened_by_trigger = if is_modal {
            None
        } else {
            components.all().iter().find_map(|(id, m)| {
                if m.component_type == "Modal"
                    && m.get_property::<String>("trigger").as_deref() == Some(node_id)
                {
                    Some(id.clone())
                } else {
                    None
                }
            })
        };

        drop(components);
        drop(proc);

        let mut open = self.open_modals.borrow_mut();
        if is_modal {
            // Toggle: insert returns false when the id was already present.
            if !open.insert(node_id.to_string()) {
                open.remove(node_id);
            }
        } else if let Some(modal_id) = opened_by_trigger {
            open.insert(modal_id);
        }
    }

    /// Load sample `idx`: reset the processor (keeping catalogs), replay its
    /// messages, refresh focus, highlight the row, and redraw. No-op if the
    /// index is out of range.
    fn select(&self, idx: usize) {
        let messages = self
            .samples
            .borrow()
            .get(idx)
            .map(|(_, msgs)| msgs.clone());
        let Some(messages) = messages else {
            return;
        };

        let mut proc = self.processor.borrow_mut();
        proc.reset();
        for msg in &messages {
            let _ = proc.process_message(msg.clone());
        }
        drop(proc);

        self.focus.borrow_mut().reset();
        self.open_modals.borrow_mut().clear();
        {
            let proc = self.processor.borrow();
            if let Some(surface) = proc.model.surfaces().next() {
                let components = surface.components.borrow();
                self.focus.borrow_mut().rebuild_from_components(&components);
            }
        }
        self.surface.set_selected_sample(idx as i32);
        self.redraw();
    }
}

/// Wrap a `Vec<LiveNode>` into the `ModelRc` shape the `nodes` property expects.
fn to_node_model(nodes: Vec<LiveNode>) -> slint::ModelRc<LiveNode> {
    slint::ModelRc::new(Rc::new(slint::VecModel::from(nodes)))
}
