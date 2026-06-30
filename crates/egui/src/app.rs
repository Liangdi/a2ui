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
use a2ui_base::protocol::common_types::DynamicString;
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
    /// Image cache: a resolved URL → `Some(decoded TextureHandle)` once decoded,
    /// or `None` for a URL attempted but failed to fetch/decode (so it isn't
    /// retried every frame). Populated by [`Self::load_images`]; cleared on
    /// sample switch. Mirrors the Iced/Bevy image caches.
    image_cache: HashMap<String, Option<egui::TextureHandle>>,
    /// Locally-tracked active tab index for Tabs components whose `activeTab`
    /// is **not** a data binding (the gallery samples fall here). Keyed by
    /// component id. Bound Tabs write to the model instead. Mirrors Iced.
    local_tabs: HashMap<String, usize>,
    /// Whether the embedded emoji icon font has been installed into the egui
    /// context yet (done once on the first frame, when a context is available).
    icons_installed: bool,
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
            image_cache: HashMap::new(),
            local_tabs: HashMap::new(),
            icons_installed: false,
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
        self.image_cache.clear();
        self.local_tabs.clear();
        self.selected_sample = idx;
    }
}

impl eframe::App for EguiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 0. One-time setup that needs an egui context: install the embedded
        //    emoji icon font (egui's default fonts have none of the Icon glyphs).
        if !self.icons_installed {
            install_icon_font(ui.ctx());
            self.icons_installed = true;
        }
        // 0b. Image pre-pass: decode any not-yet-cached Image URLs (local read +
        //     decode, or a blocking http fetch — same documented trade-off as the
        //     Bevy/Slint backends). Done before the walk so render_image only
        //     reads the cache, and so a single failed URL isn't re-fetched every
        //     frame (failures are cached as `None`).
        self.load_images(ui.ctx());

        // 1. Left-hand sample browser.
        let selected = self.selected_sample;
        let mut clicked: Option<usize> = None;
        egui::Panel::left("sample_browser")
            .default_size(240.0)
            .resizable(true)
            .show(ui, |ui| {
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
        egui::CentralPanel::default().show(ui, |ui| {
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
                image_cache: &self.image_cache,
                local_tabs: &self.local_tabs,
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
                    walk.image_cache,
                    walk.local_tabs,
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
                    &self.image_cache,
                    &self.local_tabs,
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
                    if !path.is_empty()
                        && let Some(surface) = self.processor.model.surfaces_mut().next()
                    {
                        surface.data_model.borrow_mut().set(&path, value);
                    }
                }
                PendingInteraction::TabActivate {
                    component_id,
                    index,
                } => {
                    // Unbound Tabs (the gallery samples): track the selection
                    // locally so the next frame shows the newly active panel.
                    self.local_tabs.insert(component_id, index);
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

    /// Decode every not-yet-cached `Image` component's URL into the image cache
    /// (once per URL). Mirrors Bevy's `load_images` system and Iced's
    /// `fetch_sample_images`: a read pass that collects uncached URLs while
    /// borrowing the model, then a write pass that decodes + caches each one
    /// (so the model's `Ref` is dropped before the cache is mutated). Both local
    /// files and `http(s)` URLs go through here; failures are cached as `None`
    /// so a bad URL isn't re-fetched/re-decoded every frame.
    fn load_images(&mut self, ctx: &egui::Context) {
        let urls: Vec<String> = {
            let Some(surface) = self.processor.model.surfaces().next() else {
                return;
            };
            let components = surface.components.borrow();
            let data_model = surface.data_model.borrow();
            components
                .all()
                .iter()
                .filter_map(|(id, model)| {
                    if model.component_type != "Image" {
                        return None;
                    }
                    let ctx = ComponentContext::new(
                        id.clone(),
                        surface.id.clone(),
                        &data_model,
                        &components,
                        &self.functions,
                        "",
                        None,
                    );
                    let url = model
                        .get_property::<DynamicString>("url")
                        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
                        .unwrap_or_default();
                    if url.is_empty() || self.image_cache.contains_key(&url) {
                        return None;
                    }
                    Some(url)
                })
                .collect()
        };

        // Write pass: decode each URL and cache the handle (or None on failure).
        for url in urls {
            let handle = crate::images::decode_url(&url)
                .map(|image| ctx.load_texture(&url, image, egui::TextureOptions::LINEAR));
            self.image_cache.insert(url, handle);
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
                &InputEvent::KeyPress {
                    key: InputKey::Enter,
                },
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

/// Install the embedded emoji icon font (`a2ui-icons.ttf`, the same ~12 KB
/// NotoEmoji subset the Bevy backend uses) into the egui context as a named
/// `"Icons"` family *and* as a fallback for `Proportional`/`Monospace`. egui's
/// default fonts cover none of the Icon glyphs, so `render_icon` draws the
/// emoji codepoint in the `"Icons"` family directly. Idempotent in shape but
/// should be called once (guarded by `icons_installed`) — `set_fonts` rebuilds
/// the font atlas.
fn install_icon_font(ctx: &egui::Context) {
    let bytes = include_bytes!("../assets/fonts/a2ui-icons.ttf").to_vec();
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "a2ui-icons".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    // Named family render_icon targets directly.
    fonts
        .families
        .entry(egui::FontFamily::Name(std::sync::Arc::from("Icons")))
        .or_default()
        .push("a2ui-icons".to_owned());
    // Fallback so any stray emoji codepoint elsewhere still resolves.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("a2ui-icons".to_owned());
    }
    ctx.set_fonts(fonts);
}
