//! `GpuiApp` — the GPUI view that owns the surface state and implements the
//! per-frame `Render` loop.
//!
//! This is the GPUI counterpart of the iced [`IcedApp`] and the egui
//! `EguiApp`: it owns the [`MessageProcessor`], the function map, the
//! [`FocusManager`] (a read-only shadow for parity; GPUI native focus drives
//! actual interaction), the gallery samples, the locally-tracked
//! [`open_modals`] set, the remote-image cache, and the per-component-id
//! `Entity<…State>` caches for the stateful widgets (TextField / Slider /
//! ChoicePicker).
//!
//! `render()` draws the same dark, modern chrome as the other galleries: a
//! branded sidebar sample browser + a breadcrumb-topped preview pane, then a
//! dimmed-scrim centered overlay panel for each open Modal (layered as absolute
//! children on top of the root). Widget interactions are `cx.listener` closures
//! that fire after the render borrow is released and mutate the runtime
//! directly — no egui-style collect-then-apply buffer.
//!
//! [`IcedApp`]: a2ui_iced::IcedApp
//! [`open_modals`]: GpuiApp::open_modals

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::components::dispatch_event;
use a2ui_base::event::{EventResult, InputEvent, InputKey};
use a2ui_base::focus::FocusManager;
use a2ui_base::interaction::apply_event_result;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::client_to_server::{
    ActionData, ActionPayload, ClientMessage, ClientPayload,
};
use a2ui_base::protocol::server_to_client::A2uiMessage;

use gpui_kit::component::TitleBar;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::component::separator::Separator;
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, Context, Entity, Image, Render, Subscription, div, px};
use serde_json::Value;

use crate::components::{ChoiceOption, Walk};
use crate::theme;
use crate::walker::render_node;

/// The GPUI app view — owns all runtime state, implements GPUI's `Render` trait
/// (rebuilt each frame).
pub struct GpuiApp {
    processor: MessageProcessor,
    reactive: crate::reactive::ReactiveState,
    empty_functions: HashMap<String, Box<dyn FunctionImplementation>>,
    pending_actions: Vec<ClientMessage>,
    functions: HashMap<String, Box<dyn FunctionImplementation>>,
    focus: FocusManager,
    samples: Vec<(String, Vec<A2uiMessage>)>,
    selected_sample: usize,
    open_modals: HashSet<String>,
    /// Remote-image cache: a resolved URL → its decoded gpui [`Image`] once the
    /// background fetch completes (`None` = attempted but failed, so it isn't
    /// refetched). Cleared on sample switch. Mirrors the iced/egui image caches.
    pub(crate) image_cache: HashMap<String, Option<Arc<Image>>>,
    /// URLs a background fetch is already in flight for (so a not-yet-decoded
    /// image isn't refetched every frame).
    pub(crate) image_pending: RefCell<HashSet<String>>,
    /// Locally-tracked active tab index for Tabs components whose `activeTab`
    /// is **not** a data binding (the gallery samples fall here). Keyed by
    /// component id. Bound Tabs write to the model instead.
    pub(crate) local_tabs: HashMap<String, usize>,
    /// Per-component-id `InputState` for TextField / DateTimeInput.
    pub(crate) text_states: RefCell<HashMap<String, Entity<InputState>>>,
    /// Per-component-id `SliderState` for Slider.
    pub(crate) slider_states: RefCell<HashMap<String, Entity<SliderState>>>,
    /// Per-component-id `SelectState` for single-select ChoicePicker.
    pub(crate) select_states:
        RefCell<HashMap<String, Entity<SelectState<SearchableVec<ChoiceOption>>>>>,
    /// Keeps every lazy-create subscription alive (dropping a [`Subscription`]
    /// unsubscribes it). Cleared on sample switch alongside the state caches.
    pub(crate) subscriptions: RefCell<Vec<Subscription>>,
}

impl GpuiApp {
    /// Construct with the registered catalogs + the merged function map,
    /// mirroring the iced/egui hosts.
    pub fn new(
        catalogs: Vec<a2ui_base::catalog::Catalog>,
        functions: HashMap<String, Box<dyn FunctionImplementation>>,
    ) -> Self {
        Self {
            processor: MessageProcessor::new(catalogs),
            reactive: crate::reactive::ReactiveState::default(),
            empty_functions: HashMap::new(),
            pending_actions: Vec::new(),
            functions,
            focus: FocusManager::new(),
            samples: Vec::new(),
            selected_sample: 0,
            open_modals: HashSet::new(),
            image_cache: HashMap::new(),
            image_pending: RefCell::new(HashSet::new()),
            local_tabs: HashMap::new(),
            text_states: RefCell::new(HashMap::new()),
            slider_states: RefCell::new(HashMap::new()),
            select_states: RefCell::new(HashMap::new()),
            subscriptions: RefCell::new(Vec::new()),
        }
    }

    /// Apply one incoming protocol message. Hosts should call this inside an
    /// entity update and notify their GPUI context after a successful update.
    /// This adapter displays the first surface; use one view per surface when
    /// embedding independent surfaces.
    pub fn process_message(&mut self, message: A2uiMessage) -> a2ui_base::error::Result<()> {
        self.processor.process_message(message)?;
        self.clear_widget_state();
        Ok(())
    }

    /// Read the protocol runtime, for example to consume emitted user actions.
    pub fn processor(&self) -> &MessageProcessor {
        &self.processor
    }

    /// Access opt-in evaluation/check subscriptions and their diagnostics.
    /// After directly mutating model data, invalidate the affected surface's
    /// evaluations and notify the GPUI entity. Prefer `process_message` for wire updates.
    pub fn processor_mut(&mut self) -> &mut MessageProcessor {
        &mut self.processor
    }

    /// Drain events to send back to the agent after user interactions.
    pub fn drain_outgoing_messages(
        &mut self,
    ) -> Vec<a2ui_base::protocol::client_to_server::ClientMessage> {
        let mut messages = self.processor.drain_outgoing();
        messages.append(&mut self.pending_actions);
        messages
    }

    fn clear_widget_state(&mut self) {
        self.open_modals.clear();
        self.local_tabs.clear();
        self.text_states.borrow_mut().clear();
        self.slider_states.borrow_mut().clear();
        self.select_states.borrow_mut().clear();
        self.subscriptions.borrow_mut().clear();
    }

    /// Populate the sample browser with `(name, messages)` pairs and load the
    /// sample at `initial`. Pressing a sidebar entry switches samples live.
    pub fn set_samples(&mut self, samples: Vec<(String, Vec<A2uiMessage>)>, initial: usize) {
        self.samples = samples;
        self.load_sample(initial);
    }

    /// Return agent RPC responses for the embedding host to correlate by call ID.
    pub fn drain_agent_function_responses(
        &mut self,
    ) -> Vec<a2ui_base::protocol::rpc::FunctionResponse> {
        self.processor.drain_agent_function_responses()
    }

    /// Load sample `idx`: reset the processor (keeping catalogs), replay its
    /// messages, refresh focus, clear modals + caches. No-op if out of range.
    fn load_sample(&mut self, idx: usize) {
        let Some(messages) = self.samples.get(idx).map(|(_, m)| m.clone()) else {
            return;
        };
        self.processor.reset();
        self.pending_actions.clear();
        for msg in &messages {
            let _ = self.processor.process_message(msg.clone());
        }
        self.focus.reset();
        if let Some(surface) = self.processor.model.surfaces().next() {
            let components = surface.components.borrow();
            self.focus.rebuild_from_components(&components);
        }
        self.open_modals.clear();
        // Drop the previous sample's decoded images + widget state so caches
        // don't grow unbounded / leak across many sample switches.
        self.image_cache.clear();
        self.image_pending.borrow_mut().clear();
        self.local_tabs.clear();
        self.text_states.borrow_mut().clear();
        self.slider_states.borrow_mut().clear();
        self.select_states.borrow_mut().clear();
        self.subscriptions.borrow_mut().clear();
        self.selected_sample = idx;
    }

    /// Write a value to the current surface's data model at absolute JSON
    /// Pointer `path` (empty path is an unbound widget's no-op write-back).
    pub(crate) fn set_data_value(&mut self, path: &str, value: Value) {
        if path.is_empty() {
            return;
        }
        let surface_id = if let Some(surface) = self.processor.model.surfaces_mut().next() {
            surface.data_model.borrow_mut().set(path, value);
            Some(surface.id.clone())
        } else {
            None
        };
        if let Some(id) = surface_id {
            self.processor.invalidate_evaluations(&id);
        }
    }

    // -----------------------------------------------------------------------
    // Render
    // -----------------------------------------------------------------------

    /// Build the current UI: a branded sidebar + the breadcrumb-topped preview
    /// pane, with any open Modals layered on top as absolute overlays (each
    /// behind a dimmed click-to-dismiss scrim).
    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // ── Brand header ───────────────────────────────────────────────────
        let mark = div()
            .text_color(theme::ACCENT)
            .text_size(px(18.))
            .child("◆");
        let title = v_flex()
            .child(
                div()
                    .text_color(theme::TEXT)
                    .text_size(px(15.))
                    .child("A2UI"),
            )
            .child(
                div()
                    .text_color(theme::SUBTEXT1)
                    .text_size(px(11.))
                    .child("GPUI Kit Gallery"),
            );
        let brand = h_flex()
            .gap_2p5()
            .items_center()
            .w_full()
            .child(mark)
            .child(title);

        // ── Section label ──────────────────────────────────────────────────
        let section = div()
            .text_color(theme::SUBTEXT1)
            .text_size(px(10.))
            .child("SAMPLES");

        // ── Sample rows ────────────────────────────────────────────────────
        let mut list = v_flex().gap_1();
        for (i, (name, _)) in self.samples.iter().enumerate() {
            let is_sel = i == self.selected_sample;
            let idx_color = if is_sel {
                theme::ACCENT
            } else {
                theme::SUBTEXT1
            };
            let name_color = if is_sel { theme::TEXT } else { theme::SUBTEXT0 };
            let mut row = h_flex()
                .gap_2p5()
                .items_center()
                .w_full()
                .px_3()
                .py_2()
                .rounded_md()
                .id(gpui_kit::SharedString::from(format!("sample-{i}")))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.load_sample(i);
                    cx.notify();
                }))
                .child(
                    div()
                        .text_color(idx_color)
                        .text_size(px(11.))
                        .w(px(20.))
                        .child(format!("{:>2}", i + 1)),
                )
                .child(
                    div()
                        .text_color(name_color)
                        .text_size(px(13.))
                        .child(name.clone()),
                );
            if is_sel {
                row = row.bg(theme::ACCENT_WASH);
            }
            list = list.child(row);
        }

        // ── Footer ─────────────────────────────────────────────────────────
        let footer = div()
            .text_color(theme::SUBTEXT1)
            .text_size(px(10.))
            .child(format!("{} samples", self.samples.len()));

        v_flex()
            .size_full()
            .bg(theme::MANTLE)
            .p_4()
            .gap_3()
            .w(px(248.))
            .child(brand)
            .child(Separator::horizontal().color(theme::LINE))
            .child(section)
            .child(
                div()
                    .id("sample-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(Separator::horizontal().color(theme::LINE))
            .child(footer)
            .into_any_element()
    }

    /// The main pane — a breadcrumb top bar over the rendered preview surface.
    fn render_main(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) -> AnyElement {
        let (sel, count) = (self.selected_sample, self.samples.len());
        let name = self
            .samples
            .get(sel)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();

        let crumb = div()
            .text_color(theme::SUBTEXT1)
            .text_size(px(12.))
            .child("Preview");
        let sep = div()
            .text_color(theme::SUBTEXT1)
            .text_size(px(12.))
            .child("›");
        let title = div().text_color(theme::TEXT).text_size(px(14.)).child(name);
        let chip = div()
            .bg(theme::ACCENT_WASH)
            .rounded_md()
            .px_2()
            .py_1()
            .text_color(theme::ACCENT)
            .text_size(px(11.))
            .child(format!("{} / {count}", sel + 1));

        let bar = h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(crumb)
            .child(sep)
            .child(title)
            .child(div().flex_1())
            .child(chip);
        let top_bar = div().w_full().px_5().py_3p5().bg(theme::MANTLE).child(bar);

        let preview = div()
            .id("preview")
            .flex_1()
            .size_full()
            .overflow_y_scroll()
            .p_6()
            .child(self.render_tree("root", window, cx));

        v_flex()
            .flex_1()
            .size_full()
            .bg(theme::BASE)
            .child(top_bar)
            .child(Separator::horizontal().color(theme::LINE))
            .child(preview)
            .into_any_element()
    }

    /// Walk a component subtree into an element tree. Returns a muted
    /// placeholder when the surface / root is missing.
    fn render_tree(
        &self,
        root_id: &str,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(surface) = self.processor.model.surfaces().next() else {
            return div()
                .text_color(theme::SUBTEXT1)
                .child("No surface loaded.")
                .into_any_element();
        };
        if !surface.components.borrow().contains(root_id) {
            return div()
                .text_color(theme::SUBTEXT1)
                .child("No root component")
                .into_any_element();
        }

        let data_model = surface.data_model.borrow();
        let components = surface.components.borrow();
        let focused_id = self.focus.focused_id().map(str::to_string);
        let walk = Walk {
            render_path: RefCell::new(Vec::new()),
            surface_id: &surface.id,
            catalog_id: &surface.catalog_id,
            current_protocol: surface.current_protocol,
            processor: &self.processor,
            reactive: &self.reactive,
            empty_functions: &self.empty_functions,
            data_model: &data_model,
            components: &components,
            functions: &self.functions,
            focused_id: focused_id.as_deref(),
            image_cache: &self.image_cache,
            image_pending: &self.image_pending,
            local_tabs: &self.local_tabs,
            text_states: &self.text_states,
            slider_states: &self.slider_states,
            select_states: &self.select_states,
            subscriptions: &self.subscriptions,
        };
        render_node(root_id, "", &walk, window, cx)
    }

    /// One open Modal's `content` subtree in a centered elevated panel with a
    /// title row + close button, layered over a dimmed click-to-dismiss scrim
    /// (built as a sibling absolute layer behind the panel). Returns `None` if
    /// the modal / its content is missing.
    fn render_modal_overlay(
        &mut self,
        modal_id: &str,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let surface = self.processor.model.surfaces().next()?;

        // Resolve the modal's content id + optional title in one borrow.
        let (content_id, title): (Option<String>, String) = {
            let components = surface.components.borrow();
            let m = components.get(modal_id)?;
            if m.component_type != "Modal" {
                return None;
            }
            let content = m.get_property::<String>("content");
            let title = m
                .get_property::<String>("title")
                .unwrap_or_else(|| "Dialog".to_string());
            (content, title)
        };
        let content_id = content_id?;

        let content_tree = {
            let data_model = surface.data_model.borrow();
            let components = surface.components.borrow();
            let focused_id = self.focus.focused_id().map(str::to_string);
            let walk = Walk {
                render_path: RefCell::new(Vec::new()),
                surface_id: &surface.id,
                catalog_id: &surface.catalog_id,
                current_protocol: surface.current_protocol,
                processor: &self.processor,
                reactive: &self.reactive,
                empty_functions: &self.empty_functions,
                data_model: &data_model,
                components: &components,
                functions: &self.functions,
                focused_id: focused_id.as_deref(),
                image_cache: &self.image_cache,
                image_pending: &self.image_pending,
                local_tabs: &self.local_tabs,
                text_states: &self.text_states,
                slider_states: &self.slider_states,
                select_states: &self.select_states,
                subscriptions: &self.subscriptions,
            };
            render_node(&content_id, "", &walk, window, cx)
        };

        // ── Panel chrome ───────────────────────────────────────────────────
        let close_id = gpui_kit::SharedString::from(format!("modal-close-{modal_id}"));
        let close = Button::new(close_id)
            .ghost()
            .label("✕")
            .on_click(cx.listener({
                let mid = modal_id.to_string();
                move |this, _: &ClickEvent, _window, cx| {
                    this.open_modals.remove(&mid);
                    cx.notify();
                }
            }));
        let title_row = h_flex()
            .w_full()
            .items_center()
            .child(
                div()
                    .text_color(theme::TEXT)
                    .text_size(px(14.))
                    .child(title),
            )
            .child(div().flex_1())
            .child(close);
        let panel_body = v_flex()
            .gap_3p5()
            .w_full()
            .child(title_row)
            .child(content_tree);
        let panel = div()
            .bg(theme::MANTLE)
            .border_1()
            .border_color(theme::EDGE)
            .rounded_lg()
            .p_6()
            .w(px(480.))
            .child(panel_body);

        // Scrim fills the viewport behind the panel; clicking it dismisses.
        let backdrop_id = gpui_kit::SharedString::from(format!("modal-backdrop-{modal_id}"));
        let backdrop = div()
            .absolute()
            .size_full()
            .bg(theme::SCRIM)
            .id(backdrop_id)
            .on_click(cx.listener({
                let mid = modal_id.to_string();
                move |this, _: &ClickEvent, _window, cx| {
                    this.open_modals.remove(&mid);
                    cx.notify();
                }
            }));

        Some(
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(backdrop)
                .child(div().relative().child(panel))
                .into_any_element(),
        )
    }

    // -----------------------------------------------------------------------
    // Activation (Button press → action / Modal open-close)
    // -----------------------------------------------------------------------

    /// A node was activated (button press): dispatch `Enter` via the shared core
    /// logic, apply the result, then resolve any local Modal state change.
    /// Ported from `crates/iced/src/app.rs::handle_activate`.
    pub(crate) fn handle_activate(&mut self, node_id: &str, base_path: &str) {
        self.reactive.refresh(&mut self.processor);
        if let Some(surface) = self.processor.model.surfaces().next()
            && let Some(view) = self.reactive.view(&surface.id, node_id, base_path)
            && !view.checks_valid
        {
            return;
        }
        let current_action = self
            .processor
            .model
            .surfaces()
            .next()
            .filter(|surface| surface.current_protocol)
            .map(|surface| {
                let surface_id = surface.id.clone();
                let result = {
                    let components = surface.components.borrow();
                    components
                        .get(node_id)
                        .ok_or_else(|| "Action component not found".to_string())
                        .and_then(|component| {
                            crate::walker::check_native_catalog(
                                &self.processor,
                                &surface.catalog_id,
                                component,
                            )
                        })
                };
                (surface_id, result)
            });
        if let Some((surface_id, result)) = current_action {
            let result = result.and_then(|()| {
                self.processor
                    .activate_component(&surface_id, node_id, base_path)
                    .map_err(|e| e.to_string())
            });
            if let Err(message) = result {
                self.pending_actions.push(ClientMessage {
                    version: "1.0".into(),
                    payload: ClientPayload::Error(
                        a2ui_base::protocol::client_to_server::ErrorPayload {
                            error: a2ui_base::protocol::client_to_server::ErrorData {
                                code: "EXPRESSION_ERROR".into(),
                                message,
                                surface_id: Some(surface_id),
                                function_call_id: None,
                            },
                        },
                    ),
                });
            }
            self.apply_modal_interaction(node_id);
            return;
        }

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
                base_path,
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

        if let Some(EventResult::Action {
            event_name,
            context,
            want_response,
            response_path,
        }) = result
        {
            let surface_id = self.processor.model.surfaces().next().map(|s| s.id.clone());
            if let Some(surface_id) = surface_id {
                let action_id = want_response.then(|| uuid::Uuid::new_v4().to_string());
                // Deserialize the shared wire fields so this adapter also builds
                // with cores that add optional action metadata. All user-derived
                // content remains JSON data rather than interpolated JSON text.
                let action = serde_json::from_value::<ActionData>(serde_json::json!({
                    "name": event_name,
                    "surfaceId": surface_id,
                    "sourceComponentId": node_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "context": context,
                    "wantResponse": want_response.then_some(true),
                    "actionId": action_id,
                }));
                match action {
                    Ok(action) => {
                        if let Some(id) = &action.action_id {
                            let _ = self.processor.register_action(
                                &action.surface_id,
                                id,
                                response_path,
                            );
                        }
                        // These are the existing draft catalog's event actions.
                        // Current agent-function actions need the core migration.
                        self.pending_actions.push(ClientMessage {
                            version: "v1.0".into(),
                            payload: ClientPayload::Action(ActionPayload { action }),
                        });
                    }
                    Err(error) => {
                        eprintln!("Unable to encode A2UI action: {error}");
                    }
                }
            }
        } else if let Some(result) = result {
            let _ = apply_event_result(&mut self.processor, result);
        }
        self.apply_modal_interaction(node_id);
    }

    /// Resolve a node activation into a local Modal state change. Activating a
    /// component that is some Modal's `trigger` opens that Modal; activating a
    /// Modal node directly toggles it closed. Ported from the iced/egui hosts.
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
                if self.open_modals.insert(node_id.to_string()) {
                    return; // was closed → now open
                }
                Some(node_id.to_string()) // was open → close
            } else {
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

impl Render for GpuiApp {
    fn render(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.reactive.refresh(&mut self.processor);
        // Deterministic overlay order: iterate modals sorted by id. Collected
        // up front so each overlay is built as a discrete sequential borrow.
        let mut modal_ids: Vec<String> = self.open_modals.iter().cloned().collect();
        modal_ids.sort();

        let sidebar = self.render_sidebar(cx);
        let main = self.render_main(window, cx);
        let mut root = h_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(theme::CRUST)
            .child(sidebar)
            .child(main);
        for modal_id in modal_ids {
            if let Some(overlay) = self.render_modal_overlay(&modal_id, window, cx) {
                root = root.child(overlay);
            }
        }
        // The `TitleBar` is the window's client-side decoration: a draggable
        // strip (move + double-click-to-maximize) carrying the OS window
        // controls (min / max / close) on Linux CSD. Without it the window
        // has no titlebar at all under GNOME Wayland (which ignores
        // server-side decoration requests).
        v_flex()
            .size_full()
            .child(
                TitleBar::new().child(
                    div()
                        .text_color(theme::SUBTEXT1)
                        .text_size(px(12.))
                        .child("A2UI · GPUI Kit Gallery"),
                ),
            )
            .child(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn app() -> GpuiApp {
        GpuiApp::new(vec![], HashMap::new())
    }

    fn message(value: Value) -> A2uiMessage {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn button_actions_preserve_template_context_and_are_drained() {
        let mut app = app();
        for value in [
            json!({"version":"v1.0","createSurface":{"surfaceId":"test","catalogId":"test"}}),
            json!({"version":"v1.0","updateDataModel":{"surfaceId":"test","path":"","value":{"rows":[{"name":"first"},{"name":"second"}]}}}),
            json!({"version":"v1.0","updateComponents":{"surfaceId":"test","components":[{"id":"pick","component":"Button","action":{"event":{"name":"pick","context":{"name":{"path":"name"}}}}}]}}),
        ] {
            app.process_message(message(value)).unwrap();
        }
        app.handle_activate("pick", "/rows/1");
        let messages = app.drain_outgoing_messages();
        assert_eq!(messages.len(), 1);
        let ClientPayload::Action(payload) = &messages[0].payload else {
            panic!("expected action");
        };
        assert_eq!(payload.action.name, "pick");
        assert_eq!(payload.action.context.get("name"), Some(&json!("second")));
        assert!(app.drain_outgoing_messages().is_empty());
    }

    fn current_app(action: Value) -> GpuiApp {
        let mut app = GpuiApp::new(
            vec![a2ui_tui::catalogs::basic::build_basic_catalog()],
            HashMap::new(),
        );
        app.process_message(message(json!({"version":"1.0","createSurface":{
            "surfaceId":"test","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel":{"name":"Max"},
            "components":[{"id":"button","component":"Button","action":action}]
        }}))).unwrap();
        app
    }

    #[test]
    fn current_button_routes_remote_action_with_resolved_arguments() {
        let mut app = current_app(
            json!({"functionCall":{"@call":"submit","args":{"name":{"@path":"/name"}}}}),
        );
        app.handle_activate("button", "");
        let messages = app.drain_outgoing_messages();
        assert_eq!(messages.len(), 1);
        let value = serde_json::to_value(&messages[0]).unwrap();
        assert_eq!(value["version"], "1.0");
        assert_eq!(
            value["callAgentFunction"]["callFunction"]["args"]["name"],
            "Max"
        );
    }

    #[test]
    fn current_event_waits_for_agent_then_emits_resolved_context() {
        let mut app = current_app(json!({"event":{"name":"selected","context":{
            "label":{"@call":"lookup","args":{"name":{"@path":"/name"}}}
        }}}));
        app.handle_activate("button", "");
        let requests = app.drain_outgoing_messages();
        assert_eq!(requests.len(), 1);
        let request = serde_json::to_value(&requests[0]).unwrap();
        let id = request["callAgentFunction"]["functionCallId"]
            .as_str()
            .unwrap();
        app.process_message(message(json!({"version":"1.0","agentFunctionResponse":{
            "functionCallId":id,"value":"Resolved label"
        }})))
        .unwrap();
        let output = app.drain_outgoing_messages();
        assert_eq!(output.len(), 1);
        let event = serde_json::to_value(&output[0]).unwrap();
        assert_eq!(event["action"]["context"]["label"], "Resolved label");
        assert_eq!(event["action"]["name"], "selected");
    }

    #[test]
    fn current_action_rejection_is_reported_to_host() {
        let mut app = current_app(json!({"functionCall":{"@call":"submit","catalogId":"unknown"}}));
        app.handle_activate("button", "");
        let messages = app.drain_outgoing_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(&messages[0].payload, ClientPayload::Error(_)));
    }

    #[test]
    fn native_catalog_resolution_honors_explicit_override_without_name_fallback() {
        let mut app = GpuiApp::new(
            vec![
                a2ui_tui::catalogs::basic::build_basic_catalog(),
                a2ui_tui::catalogs::minimal::build_minimal_catalog(),
                a2ui_base::catalog::Catalog::new("custom"),
            ],
            HashMap::new(),
        );
        let basic = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";
        let minimal = "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json";
        app.process_message(message(json!({"version":"1.0","createSurface":{
            "surfaceId":"test","catalogId":basic,"components":[
                {"id":"supported","component":"Text","text":"ok","@catalogId":minimal},
                {"id":"collision","component":"Text","text":"no","@catalogId":"custom"},
                {"id":"unknown","component":"Text","text":"no","@catalogId":"unknown"}
            ]
        }})))
        .unwrap();
        let surface = app.processor.model.get_surface("test").unwrap();
        let components = surface.components.borrow();
        assert!(
            crate::walker::check_native_catalog(
                &app.processor,
                basic,
                components.get("supported").unwrap()
            )
            .is_ok()
        );
        for id in ["collision", "unknown"] {
            assert!(
                crate::walker::check_native_catalog(
                    &app.processor,
                    basic,
                    components.get(id).unwrap()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn native_data_edits_recompute_automatic_current_checks() {
        let mut app = GpuiApp::new(
            vec![a2ui_tui::catalogs::basic::build_basic_catalog()],
            HashMap::new(),
        );
        app.process_message(message(json!({"version":"1.0","createSurface":{
            "surfaceId":"s","catalogId":"https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "dataModel":{"name":""},"components":[{"id":"root","component":"TextField","value":{"@path":"/name"},
            "@checks":[{"condition":{"@call":"required","args":{"value":{"@path":"/name"}}},"message":"Required"}]}]
        }}))).unwrap();
        app.reactive.refresh(&mut app.processor);
        assert!(!app.reactive.view("s", "root", "").unwrap().checks_valid);
        app.set_data_value("/name", json!("Ada"));
        app.reactive.refresh(&mut app.processor);
        assert!(app.reactive.view("s", "root", "").unwrap().checks_valid);
    }

    #[test]
    fn accepts_streamed_surface_lifecycle() {
        let mut app = app();
        app.process_message(message(json!({"version":"v1.0","createSurface":{
            "surfaceId":"test", "catalogId":"test"
        }})))
        .unwrap();
        assert_eq!(app.processor().model.surfaces().count(), 1);
        app.process_message(message(
            json!({"version":"v1.0","deleteSurface":{"surfaceId":"test"}}),
        ))
        .unwrap();
        assert_eq!(app.processor().model.surfaces().count(), 0);
    }

    #[test]
    fn streaming_updates_invalidate_local_widget_state() {
        let mut app = app();
        app.local_tabs.insert("old".into(), 2);
        app.open_modals.insert("old".into());
        app.process_message(message(json!({"version":"v1.0","createSurface":{
            "surfaceId":"test", "catalogId":"test"
        }})))
        .unwrap();
        assert!(app.local_tabs.is_empty());
        assert!(app.open_modals.is_empty());
    }

    #[test]
    fn invalid_sample_selection_does_not_reset_runtime() {
        let mut app = app();
        app.process_message(message(json!({"version":"v1.0","createSurface":{
            "surfaceId":"test", "catalogId":"test"
        }})))
        .unwrap();
        app.load_sample(99);
        assert_eq!(app.processor().model.surfaces().count(), 1);
    }
}
