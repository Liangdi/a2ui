//! Shared runtime state — the Bevy resource owning the A2UI processor, the
//! stable `component_id → Entity` map the reconciler diffs against, and the
//! collected-then-applied interaction queue.
//!
//! This is the Bevy counterpart of egui's `EguiApp` fields (processor, functions,
//! focus, samples, open_modals) plus the `node_map` that retained-mode Bevy
//! uniquely needs. It is a plain `Resource` accessed via `ResMut` — `ResMut`
//! gives `&mut` so no `RefCell` is needed for the resource itself (the
//! processor's *internal* `RefCell`s, e.g. `surface.components.borrow()`, are
//! borrowed through exactly as the egui/slint backends do).

use std::collections::{HashMap, HashSet};

use bevy::ecs::{component::Component, entity::Entity};

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::focus::FocusManager;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::protocol::server_to_client::A2uiMessage;

use crate::interaction::PendingInteraction;

/// Marker component on every Bevy entity that mirrors an A2UI component.
///
/// Carries the A2UI `component_id` so interaction-collection systems can map a
/// Bevy `Entity` (the source of a widget event) back to its A2UI component, and
/// the reconciler can map an A2UI component id to its Bevy entity (via the
/// `node_map` in [`A2uiState`]).
#[derive(Component, Debug, Clone)]
pub struct A2uiNode {
    /// The A2UI component id this entity mirrors (stable across frames).
    pub id: String,
    /// The A2UI component_type (e.g. "Text", "Button") this entity was spawned
    /// to mirror. The reconciler compares this against the planned kind: if the
    /// same id now maps to a *different* type (common on sample switch — both
    /// samples have an id "root" but it may be Text in one, Row in another), it
    /// despawns + respawns the entity so stale components from the old type
    /// don't linger and visually stack with the new type.
    pub kind: String,
    /// True for entities mounted under the top-level overlay (Modal content).
    /// Lets the reconciler clear the overlay wholesale on sample switch.
    pub overlay: bool,
}

/// One pending interaction collected during a frame's widget events, applied by
/// [`crate::interaction::apply_interactions`] after the borrows are settled.
///
/// (Defined in [`crate::interaction`]; re-aliased here only for doc links.)

/// The shared runtime state — owned as a Bevy **`NonSend` resource**, one per app.
///
/// It is `NonSend` (not `Resource`) because the wrapped `MessageProcessor`
/// holds `RefCell`-backed model maps that are `!Sync`, so it cannot satisfy
/// Bevy's `Send + Sync` resource requirement. Systems access it via
/// `NonSendMut<A2uiState>`; observers via `DeferredWorld::non_send_resource_mut`.
/// Single-threaded access is fine — only one system touches it per tick.
///
/// Construct from the gallery (or any host) via [`A2uiState::new`], insert as a
/// non-send resource (`app.insert_non_send_resource(...)`), and add
/// [`crate::A2uiPlugin`].
pub struct A2uiState {
    /// The A2UI message processor — owns catalogs + the live component tree.
    pub processor: MessageProcessor,
    /// Merged function map (catalog functions passed separately — the processor
    /// owns the catalogs and doesn't expose their functions). Mirrors egui/slint.
    pub functions: HashMap<String, Box<dyn FunctionImplementation>>,
    /// Focus manager — read-only shadow for focus-ring styling; Bevy's native
    /// focus drives actual interaction (as with egui/slint).
    pub focus: FocusManager,
    /// `(name, messages)` pairs for the sample browser.
    pub samples: Vec<(String, Vec<A2uiMessage>)>,
    /// Currently-selected sample index.
    pub selected_sample: usize,
    /// The selection the sample-browser rows were last rendered for. The
    /// `update_browser` system rebuilds rows when this differs from
    /// `selected_sample`, so the highlight follows clicks.
    pub browser_last_selection: Option<usize>,
    /// Locally-tracked open-Modal ids (the gallery has no server to flip a
    /// Modal's `isOpen`). Mirrors egui's `open_modals`.
    pub open_modals: HashSet<String>,
    /// Stable map: A2UI component id → the Bevy entity mirroring it. The heart
    /// of the reconciler — entity identity is preserved across frames so widget
    /// state (slider drag, checkbox, text-input cursor) survives.
    pub node_map: HashMap<String, Entity>,
    /// Shadow of `node_map`: A2UI component id → the component_type the entity
    /// was spawned to mirror. The reconciler compares this against the planned
    /// kind; if the same id now maps to a different type (common on sample
    /// switch — "root" may be Text in one sample, Row in another), it despawns
    /// + respawns so stale components from the old type don't stack visually.
    pub kind_map: HashMap<String, String>,
    /// The root surface entity (the central pane's scroll container), parented
    /// by the reconciler's top-level nodes.
    pub surface_root: Option<Entity>,
    /// The overlay root entity (parents all open-Modal content subtrees).
    pub overlay_root: Option<Entity>,
    /// Bumped whenever the tree structure may have changed (message processed,
    /// sample switched, interaction applied) so the reconciler knows to
    /// spawn/despawn/reorder. Property re-resolution happens every frame
    /// regardless.
    pub dirty: bool,
}

impl A2uiState {
    /// Construct with the registered catalogs + the merged function map.
    pub fn new(
        catalogs: Vec<a2ui_base::catalog::Catalog>,
        functions: HashMap<String, Box<dyn FunctionImplementation>>,
    ) -> Self {
        Self {
            processor: MessageProcessor::new(catalogs),
            functions,
            focus: FocusManager::new(),
            samples: Vec::new(),
            selected_sample: 0,
            browser_last_selection: None,
            open_modals: HashSet::new(),
            node_map: HashMap::new(),
            kind_map: HashMap::new(),
            surface_root: None,
            overlay_root: None,
            dirty: true,
        }
    }

    /// Populate the sample browser and load the sample at `initial`.
    pub fn set_samples(&mut self, samples: Vec<(String, Vec<A2uiMessage>)>, initial: usize) {
        self.samples = samples;
        self.load_sample(initial);
    }

    /// Feed an A2UI message into the processor, then mark dirty + refresh focus.
    pub fn process_message(&mut self, message: A2uiMessage) {
        let _ = self.processor.process_message(message);
        self.rebuild_focus();
        self.dirty = true;
    }

    /// Rebuild the focus list from the current component tree.
    pub fn rebuild_focus(&mut self) {
        if let Some(surface) = self.processor.model.surfaces().next() {
            let components = surface.components.borrow();
            self.focus.rebuild_from_components(&components);
        }
    }

    /// Load sample `idx`: reset the processor (keeping catalogs), replay its
    /// messages, refresh focus, clear the open-modals set, and mark dirty so
    /// the reconciler respawns the whole tree. No-op if out of range.
    pub fn load_sample(&mut self, idx: usize) {
        let Some(messages) = self.samples.get(idx).map(|(_, m)| m.clone()) else {
            return;
        };
        self.processor.reset();
        for msg in &messages {
            let _ = self.processor.process_message(msg.clone());
        }
        self.focus.reset();
        if let Some(surface) = self.processor.model.surfaces().next() {
            let components = surface.components.borrow();
            self.focus.rebuild_from_components(&components);
        }
        self.open_modals.clear();
        self.dirty = true;
        self.selected_sample = idx;
    }
}

/// Resource holding the per-frame interaction queue. `NonSend` for the same
/// reason as [`A2uiState`] (it holds `PendingInteraction` which may wrap
/// non-Send data via the processor). Separate from `A2uiState` so observers can
/// push to it via `DeferredWorld::non_send_resource_mut` without going through
/// the processor. Accessed as `NonSendMut<PendingInteractions>`.
#[derive(Default)]
pub struct PendingInteractions(pub Vec<PendingInteraction>);
