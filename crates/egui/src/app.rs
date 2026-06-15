//! `EguiApp` — the [`eframe::App`] that owns the surface state and drives the
//! immediate-mode render loop.
//!
//! This is the egui counterpart of the Slint host ([`crate`]'s sibling's
//! `SurfaceHost`): it owns the [`MessageProcessor`], the function map, the
//! [`FocusManager`] (kept as a read-only shadow for focus-ring styling; egui
//! native focus drives actual interaction), the gallery samples, the persistent
//! [`EditBuffers`], and the locally-tracked [`open_modals`] set (the gallery has
//! no server to flip a Modal's `isOpen`).
//!
//! `update()` draws a left-hand sample browser (`SidePanel`) + the rendered
//! surface (`CentralPanel`), then a top-most `egui::Window` for each open Modal.
//! Widget interactions are collected into a `Vec<PendingInteraction>` during the
//! walk and applied by [`apply_pending`] after — because the surface's data
//! model / components are borrowed for the whole walk.

use std::collections::{HashMap, HashSet};

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::components::dispatch_event;
use a2ui_base::event::{InputEvent, InputKey};
use a2ui_base::focus::FocusManager;
use a2ui_base::interaction::apply_event_result;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::server_to_client::A2uiMessage;

use crate::components::Walk;
use crate::edit_state::EditBuffers;
use crate::interaction::PendingInteraction;
use crate::walker::render_node;

/// The egui app — owns all state, drives the immediate-mode render loop.
pub struct EguiApp {
    processor: MessageProcessor,
    functions: HashMap<String, Box<dyn FunctionImplementation>>,
    focus: FocusManager,
    samples: Vec<(String, Vec<A2uiMessage>)>,
    selected_sample: usize,
    edit_buffers: EditBuffers,
    open_modals: HashSet<String>,
}

impl EguiApp {
    /// Construct with the registered catalogs + the merged function map.
    ///
    /// `functions` is the merged function map (the same implementations the
    /// `catalogs` carry — passed separately because [`MessageProcessor`] owns
    /// the catalogs and doesn't expose their functions), mirroring the Slint host.
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
            edit_buffers: EditBuffers::default(),
            open_modals: HashSet::new(),
        }
    }

    /// Populate the sample browser with `(name, messages)` pairs and load the
    /// sample at `initial`. Clicking a sidebar row switches samples live.
    pub fn set_samples(&mut self, samples: Vec<(String, Vec<A2uiMessage>)>, initial: usize) {
        self.samples = samples;
        self.load_sample(initial);
    }

    /// Feed an A2UI message into the processor, then refresh focus + repaint.
    pub fn process_message(&mut self, message: A2uiMessage) {
        let _ = self.processor.process_message(message);
        self.rebuild_focus();
    }

    /// Rebuild the focus list from the current component tree.
    pub fn rebuild_focus(&mut self) {
        if let Some(surface) = self.processor.model.surfaces().next() {
            let components = surface.components.borrow();
            self.focus.rebuild_from_components(&components);
        }
    }

    /// Load sample `idx`: reset the processor (keeping catalogs), replay its
    /// messages, refresh focus, invalidate edit buffers, clear modals. No-op if
    /// the index is out of range.
    fn load_sample(&mut self, idx: usize) {
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
        self.edit_buffers.invalidate();
        self.open_modals.clear();
        self.selected_sample = idx;
    }
}

impl eframe::App for EguiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 1. Left-hand sample browser.
        let selected = self.selected_sample;
        let mut clicked: Option<usize> = None;
        egui::Panel::left("sample_browser")
            .default_size(240.0)
            .resizable(true)
            .show_inside(ui, |ui| {
                ui.heading("Samples");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, (name, _)) in self.samples.iter().enumerate() {
                        let is_sel = i == selected;
                        if ui.selectable_label(is_sel, name).clicked() {
                            clicked = Some(i);
                        }
                    }
                });
            });
        if let Some(i) = clicked {
            self.load_sample(i);
        }

        // 2. Surface pane — walk the tree, collecting interactions.
        //    The closure captures &mut self, so each `self.<field>` access is a
        //    distinct field borrow (the immutable Walk fields and the mutable
        //    edit_buffers/pending coexist without conflict).
        let mut pending: Vec<PendingInteraction> = Vec::new();
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.edit_buffers.begin_frame();

            let Some(surface) = self.processor.model.surfaces().next() else {
                ui.label("No surface loaded.");
                return;
            };
            if !surface.components.borrow().contains("root") {
                ui.label("No root component");
                return;
            }

            let data_model = surface.data_model.borrow();
            let components = surface.components.borrow();
            let focused_id = self.focus.focused_id().map(str::to_string);
            let walk = Walk {
                surface_id: &surface.id,
                data_model: &data_model,
                components: &components,
                functions: &self.functions,
                focused_id: focused_id.as_deref(),
                open_modals: &self.open_modals,
            };

            egui::ScrollArea::vertical().show(ui, |ui| {
                render_node(
                    "root",
                    walk.surface_id,
                    "",
                    ui,
                    walk.data_model,
                    walk.components,
                    walk.functions,
                    walk.focused_id,
                    walk.open_modals,
                    &mut self.edit_buffers,
                    &mut pending,
                );
            });
        });

        // 3. Modal overlays — rendered top-level (each re-borrows the surface).
        let ctx = ui.ctx().clone();
        let open_modals: Vec<String> = self.open_modals.iter().cloned().collect();
        for modal_id in open_modals {
            self.render_modal_overlay(&ctx, &modal_id, &mut pending);
        }

        // 4. Apply collected interactions (surface borrows dropped).
        self.apply_pending(pending);
    }
}

impl EguiApp {
    /// Render one open Modal's `content` subtree inside a floating `egui::Window`.
    ///
    /// Re-borrows the surface from the processor itself (top-level call, outside
    /// the CentralPanel closure) so there's no conflict with the main tree's
    /// borrows.
    fn render_modal_overlay(
        &mut self,
        ctx: &egui::Context,
        modal_id: &str,
        pending: &mut Vec<PendingInteraction>,
    ) {
        let content_id = {
            let Some(surface) = self.processor.model.surfaces().next() else {
                return;
            };
            let components = surface.components.borrow();
            components.get(modal_id).and_then(|m| {
                (m.component_type == "Modal")
                    .then(|| m.get_property::<String>("content"))
                    .flatten()
            })
        };
        let Some(content_id) = content_id else { return };

        let mut open = true;
        egui::Window::new("Modal")
            .id(egui::Id::new(modal_id))
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                let Some(surface) = self.processor.model.surfaces().next() else {
                    return;
                };
                let data_model = surface.data_model.borrow();
                let components = surface.components.borrow();
                let focused_id = self.focus.focused_id().map(str::to_string);
                render_node(
                    &content_id,
                    &surface.id,
                    "",
                    ui,
                    &data_model,
                    &components,
                    &self.functions,
                    focused_id.as_deref(),
                    &self.open_modals,
                    &mut self.edit_buffers,
                    pending,
                );
            });
        if !open {
            pending.push(PendingInteraction::ModalClose {
                modal_id: modal_id.to_string(),
            });
        }
    }

    /// Apply collected interactions to the processor (mirrors the Slint host's
    /// `handle_activate` + `apply_modal_interaction`).
    fn apply_pending(&mut self, pending: Vec<PendingInteraction>) {
        for interaction in pending {
            match interaction {
                PendingInteraction::ButtonActivate { component_id } => {
                    self.handle_activate(&component_id);
                }
                PendingInteraction::DataUpdate { path, value } => {
                    if let Some(surface) = self.processor.model.surfaces_mut().next() {
                        surface.data_model.borrow_mut().set(&path, value);
                    }
                }
                PendingInteraction::Toggle { path } => {
                    if let Some(surface) = self.processor.model.surfaces_mut().next() {
                        let cur = surface
                            .data_model
                            .borrow()
                            .get(&path)
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        surface
                            .data_model
                            .borrow_mut()
                            .set(&path, serde_json::json!(!cur));
                    }
                }
                PendingInteraction::ModalTrigger { modal_id } => {
                    self.open_modals.insert(modal_id);
                }
                PendingInteraction::ModalClose { modal_id } => {
                    self.open_modals.remove(&modal_id);
                }
            }
        }
    }

    /// A node was activated (button click): dispatch `Enter` via the shared core
    /// logic, apply the result, then resolve any local Modal state change.
    /// Ported from `crates/slint/src/host.rs::handle_activate`.
    fn handle_activate(&mut self, node_id: &str) {
        let result = {
            let surface = match self.processor.model.surfaces().next() {
                Some(s) => s,
                None => return,
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
            let _ = apply_event_result(&mut self.processor, result);
        }
        self.apply_modal_interaction(node_id);
    }

    /// Resolve a node activation into a local Modal state change. Activating a
    /// component that is some Modal's `trigger` opens that Modal; activating a
    /// Modal node directly toggles it closed. Ported from the Slint host.
    fn apply_modal_interaction(&mut self, node_id: &str) {
        let modal_id = {
            let Some(surface) = self.processor.model.surfaces().next() else {
                return;
            };
            let components = surface.components.borrow();
            let is_modal = components
                .get(node_id)
                .map(|m| m.component_type == "Modal")
                .unwrap_or(false);
            if is_modal {
                // Toggle this Modal.
                if self.open_modals.insert(node_id.to_string()) {
                    return; // was closed → now open
                }
                Some(node_id.to_string()) // was open → close
            } else {
                // Opening a Modal whose trigger is this node.
                components.all().iter().find_map(|(id, m)| {
                    (m.component_type == "Modal"
                        && m.get_property::<String>("trigger").as_deref() == Some(node_id))
                    .then(|| id.clone())
                })
            }
        };

        match modal_id {
            Some(id) if id == node_id => {
                self.open_modals.remove(&id);
            }
            Some(id) => {
                self.open_modals.insert(id);
            }
            None => {}
        }
    }
}
