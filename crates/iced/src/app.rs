//! `IcedApp` — the Elm application state that owns the surface state and
//! exposes the `view` / `update` pair `iced::application` drives.
//!
//! This is the Iced counterpart of the egui [`EguiApp`] and the Slint host: it
//! owns the [`MessageProcessor`], the function map, the [`FocusManager`] (kept
//! as a read-only shadow for parity; Iced native focus drives actual
//! interaction), the gallery samples, and the locally-tracked [`open_modals`]
//! set (the gallery has no server to flip a Modal's `isOpen`).
//!
//! `view()` draws a left-hand sample browser + the rendered surface, then a
//! centered overlay panel for each open Modal (layered via a [`Stack`]).
//! Widget interactions are [`Message`]s attached in `view` and applied by
//! [`update`] — because Iced is Elm, `view` and `update` never overlap, so no
//! collect-then-apply buffer is needed (unlike the egui backend's
//! `PendingInteraction` vec).
//!
//! [`EguiApp`]: a2ui_egui::EguiApp
//! [`open_modals`]: IcedApp::open_modals
//! [`update`]: IcedApp::update

use std::collections::{HashMap, HashSet};

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::components::dispatch_event;
use a2ui_base::event::{InputEvent, InputKey};
use a2ui_base::focus::FocusManager;
use a2ui_base::interaction::apply_event_result;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::server_to_client::A2uiMessage;

use iced::widget::{Column, Stack, button, container, scrollable, text};
use iced::{Element, Fill, Length, Task};

use crate::message::Message;
use crate::walker::render_node;

/// The Iced app state — owns all runtime state, exposes the Elm
/// `view`/`update` pair.
pub struct IcedApp {
    processor: MessageProcessor,
    functions: HashMap<String, Box<dyn FunctionImplementation>>,
    focus: FocusManager,
    samples: Vec<(String, Vec<A2uiMessage>)>,
    selected_sample: usize,
    open_modals: HashSet<String>,
}

impl IcedApp {
    /// Construct with the registered catalogs + the merged function map.
    ///
    /// `functions` is the merged function map (the same implementations the
    /// `catalogs` carry — passed separately because [`MessageProcessor`] owns
    /// the catalogs and doesn't expose their functions), mirroring the egui/Slint
    /// hosts.
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
            open_modals: HashSet::new(),
        }
    }

    /// Populate the sample browser with `(name, messages)` pairs and load the
    /// sample at `initial`. Pressing a sidebar entry switches samples live.
    pub fn set_samples(&mut self, samples: Vec<(String, Vec<A2uiMessage>)>, initial: usize) {
        self.samples = samples;
        self.load_sample(initial);
    }

    /// Load sample `idx`: reset the processor (keeping catalogs), replay its
    /// messages, refresh focus, clear modals. No-op if the index is out of range.
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
        self.open_modals.clear();
        self.selected_sample = idx;
    }

    // -----------------------------------------------------------------------
    // The Elm pair
    // -----------------------------------------------------------------------

    /// Apply a widget-produced [`Message`] to the runtime state. Returns a
    /// [`Task`] (always `none` — the gallery has no async work to perform).
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ButtonActivate { component_id } => {
                self.handle_activate(&component_id);
            }
            Message::DataUpdate { path, value } => {
                // Empty path = an unbound Slider's no-op write-back (see
                // `render_slider`); ignore it instead of writing to the root.
                if !path.is_empty()
                    && let Some(surface) = self.processor.model.surfaces_mut().next()
                {
                    surface.data_model.borrow_mut().set(&path, value);
                }
            }
            Message::ModalTrigger { modal_id } => {
                self.open_modals.insert(modal_id);
            }
            Message::ModalClose { modal_id } => {
                self.open_modals.remove(&modal_id);
            }
            Message::SelectSample(idx) => {
                self.load_sample(idx);
            }
        }
        Task::none()
    }

    /// Build the current UI: a left-hand sample browser + the rendered surface,
    /// with any open Modals layered on top via a [`Stack`].
    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = self.render_sidebar();
        let main = self.render_surface();
        let content = iced::widget::row![sidebar, main]
            .spacing(0.0)
            .width(Fill)
            .height(Fill);

        if self.open_modals.is_empty() {
            content.into()
        } else {
            let mut stack = Stack::new().push(content);
            // Deterministic overlay order: iterate modals sorted by id.
            let mut modal_ids: Vec<&String> = self.open_modals.iter().collect();
            modal_ids.sort();
            for modal_id in modal_ids {
                if let Some(overlay) = self.render_modal_overlay(modal_id) {
                    stack = stack.push(overlay);
                }
            }
            stack.into()
        }
    }

    // -----------------------------------------------------------------------
    // View helpers
    // -----------------------------------------------------------------------

    /// Left-hand sample browser — a scrollable list of selectable sample names.
    fn render_sidebar(&self) -> Element<'_, Message> {
        let mut col = Column::new().spacing(3.0);
        col = col.push(text("Samples").size(16.0));
        for (i, (name, _)) in self.samples.iter().enumerate() {
            let is_sel = i == self.selected_sample;
            let btn = button(text(name.clone()))
                .on_press(Message::SelectSample(i))
                .style(if is_sel { button::primary } else { button::secondary });
            col = col.push(btn);
        }
        container(scrollable(col))
            .width(Length::Fixed(220.0))
            .height(Fill)
            .padding(8.0)
            .into()
    }

    /// The rendered surface — a scrollable walk of the `root` component tree.
    fn render_surface(&self) -> Element<'_, Message> {
        let Some(surface) = self.processor.model.surfaces().next() else {
            return text("No surface loaded.").into();
        };
        if !surface.components.borrow().contains("root") {
            return text("No root component").into();
        }

        let data_model = surface.data_model.borrow();
        let components = surface.components.borrow();
        let focused_id = self.focus.focused_id().map(str::to_string);
        let tree = render_node(
            "root",
            &surface.id,
            "",
            &data_model,
            &components,
            &self.functions,
            focused_id.as_deref(),
        );
        container(scrollable(tree)).width(Fill).height(Fill).padding(16.0).into()
    }

    /// One open Modal's `content` subtree in a centered bordered overlay panel,
    /// with a close button. Layered over the main surface by `view`'s [`Stack`].
    fn render_modal_overlay(&self, modal_id: &str) -> Option<Element<'_, Message>> {
        let surface = self.processor.model.surfaces().next()?;
        let content_id = {
            let components = surface.components.borrow();
            components.get(modal_id).and_then(|m| {
                (m.component_type == "Modal")
                    .then(|| m.get_property::<String>("content"))
                    .flatten()
            })
        }?;

        let data_model = surface.data_model.borrow();
        let components = surface.components.borrow();
        let focused_id = self.focus.focused_id().map(str::to_string);
        let content_tree = render_node(
            &content_id,
            &surface.id,
            "",
            &data_model,
            &components,
            &self.functions,
            focused_id.as_deref(),
        );

        let close = button(text("✕ Close"))
            .on_press(Message::ModalClose { modal_id: modal_id.to_string() });
        let panel = container(
            Column::new()
                .spacing(8.0)
                .push(close)
                .push(content_tree),
        )
        .padding(16.0)
        .width(Length::Fixed(440.0))
        .style(container::bordered_box);

        // Full-fill container that centers the panel over the viewport.
        Some(
            container(panel)
                .width(Fill)
                .height(Fill)
                .center_x(Fill)
                .center_y(Fill)
                .into(),
        )
    }

    // -----------------------------------------------------------------------
    // Activation (Button press → action / Modal open-close)
    // -----------------------------------------------------------------------

    /// A node was activated (button press): dispatch `Enter` via the shared core
    /// logic, apply the result, then resolve any local Modal state change.
    /// Ported from `crates/egui/src/app.rs::handle_activate`.
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
    /// Modal node directly toggles it closed. Ported from the egui/Slint hosts.
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
