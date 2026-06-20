//! `IcedApp` — the Elm application state that owns the surface state and
//! exposes the `view` / `update` pair `iced::application` drives.
//!
//! This is the Iced counterpart of the egui [`EguiApp`] and the Slint host: it
//! owns the [`MessageProcessor`], the function map, the [`FocusManager`] (kept
//! as a read-only shadow for parity; Iced native focus drives actual
//! interaction), the gallery samples, and the locally-tracked [`open_modals`]
//! set (the gallery has no server to flip a Modal's `isOpen`).
//!
//! `view()` draws a dark, modern chrome: a branded sidebar sample browser + a
//! breadcrumb-topped preview pane, then a dimmed-scrim centered overlay panel
//! for each open Modal (layered via a [`Stack`]). The palette + widget styles
//! live in [`crate::style`].
//!
//! Widget interactions are [`Message`]s attached in `view` and applied by
//! [`update`] — because Iced is Elm, `view` and `update` never overlap, so no
//! collect-then-apply buffer is needed (unlike the egui backend's
//! `PendingInteraction` vec).
//!
//! [`EguiApp`]: a2ui_egui::EguiApp
//! [`open_modals`]: IcedApp::open_modals
//! [`update`]: IcedApp::update

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::components::dispatch_event;
use a2ui_base::event::{InputEvent, InputKey};
use a2ui_base::focus::FocusManager;
use a2ui_base::interaction::apply_event_result;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::DynamicString;
use a2ui_base::protocol::server_to_client::A2uiMessage;

use iced::widget::image;
use iced::widget::{Column, Stack, button, container, rule, scrollable, text};
use iced::{Element, Fill, Font, Length, Task};

use crate::message::Message;
use crate::style;
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
    /// Remote-image cache: resolved `http(s)` URL → decoded [`image::Handle`]
    /// once its background fetch completes (`None` = attempted but failed, so
    /// it is not refetched). Populated by [`fetch_sample_images`] and read in
    /// `view` via [`crate::walker::render_node`]. Local-file images bypass it.
    image_cache: HashMap<String, Option<image::Handle>>,
    /// Locally-tracked active tab for Tabs components whose `activeTab` is not
    /// a data binding (the gallery samples). Keyed by component id. A bound
    /// Tabs writes to the model instead and never touches this.
    local_tabs: HashMap<String, usize>,
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
            image_cache: HashMap::new(),
            local_tabs: HashMap::new(),
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
        // Drop the previous sample's decoded images so the cache doesn't grow
        // unbounded across many sample switches (re-fetched on demand).
        self.image_cache.clear();
        self.local_tabs.clear();
        self.selected_sample = idx;
    }

    // -----------------------------------------------------------------------
    // Remote image loading
    // -----------------------------------------------------------------------

    /// Spawn background fetches for every remote (`http(s)`) `Image` URL in the
    /// current sample that isn't already cached. Called from boot and from
    /// [`Message::SelectSample`]. Local-file images are decoded inline in
    /// `view` (via `Handle::from_path`) and are skipped here.
    ///
    /// Each URL is resolved through a [`ComponentContext`] so a bound `url`
    /// (e.g. `{"path": "/image"}`) is dereferenced against the data model,
    /// exactly as `render_image` does it.
    pub fn fetch_sample_images(&self) -> Task<Message> {
        let Some(surface) = self.processor.model.surfaces().next() else {
            return Task::none();
        };
        let data_model = surface.data_model.borrow();
        let components = surface.components.borrow();

        let urls: Vec<String> = components
            .all()
            .iter()
            .filter_map(|(id, m)| {
                if m.component_type != "Image" {
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
                let ds = m.get_property::<DynamicString>("url")?;
                let url = ctx.data_context.resolve_dynamic_string(&ds);
                (url.starts_with("http://") || url.starts_with("https://")).then_some(url)
            })
            .filter(|url| !self.image_cache.contains_key(url))
            .collect();

        Task::batch(urls.into_iter().map(fetch_image_task))
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
                Task::none()
            }
            Message::DataUpdate { path, value } => {
                // Empty path = an unbound Slider's no-op write-back (see
                // `render_slider`); ignore it instead of writing to the root.
                if !path.is_empty()
                    && let Some(surface) = self.processor.model.surfaces_mut().next()
                {
                    surface.data_model.borrow_mut().set(&path, value);
                }
                Task::none()
            }
            Message::ImageLoaded { url, handle } => {
                self.image_cache.insert(url, Some(handle));
                Task::none()
            }
            Message::ImageLoadFailed { url } => {
                // Record the attempt so the fetch isn't retried; the
                // placeholder stays in place.
                self.image_cache.insert(url, None);
                Task::none()
            }
            Message::ModalTrigger { modal_id } => {
                self.open_modals.insert(modal_id);
                Task::none()
            }
            Message::TabActivate {
                component_id,
                index,
            } => {
                // Unbound Tabs (the gallery samples): track the selection
                // locally so the next `view` shows the newly active panel.
                self.local_tabs.insert(component_id, index);
                Task::none()
            }
            Message::ModalClose { modal_id } => {
                self.open_modals.remove(&modal_id);
                Task::none()
            }
            Message::SelectSample(idx) => {
                self.load_sample(idx);
                self.fetch_sample_images()
            }
        }
    }

    /// Build the current UI: a branded sidebar + the breadcrumb-topped preview
    /// pane, with any open Modals layered on top via a [`Stack`] (each behind a
    /// dimmed scrim).
    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = self.render_sidebar();
        let main = self.render_main();
        let content = iced::widget::row![sidebar, main]
            .spacing(0.0)
            .width(Fill)
            .height(Fill);
        // Paint the crust backdrop behind the whole window so any sub-pixel gap
        // between sidebar / preview reads as intentional rather than white.
        let content = container(content)
            .style(style::app_bg)
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

    /// Left-hand sample browser — a branded header, a scrollable list of
    /// selectable sample rows (index + name), and a count footer.
    fn render_sidebar(&self) -> Element<'_, Message> {
        // ── Brand header ───────────────────────────────────────────────────
        let mark = text("◆").color(style::ACCENT).size(18.0);
        let title = iced::widget::column![
            text("A2UI").size(15.0).color(style::TEXT),
            text("Iced Gallery").size(11.0).color(style::SUBTEXT1),
        ]
        .spacing(0.0);
        let brand = iced::widget::row![mark, title]
            .spacing(10.0)
            .align_y(iced::alignment::Vertical::Center)
            .width(Fill);
        let header = container(brand).width(Fill).padding([2.0, 0.0]);

        // ── Section label ──────────────────────────────────────────────────
        let section = text("SAMPLES")
            .size(10.0)
            .color(style::SUBTEXT1)
            .font(Font::MONOSPACE);

        // ── Sample rows ────────────────────────────────────────────────────
        let mut list = Column::new().spacing(4.0);
        for (i, (name, _)) in self.samples.iter().enumerate() {
            let is_sel = i == self.selected_sample;
            let idx_color = if is_sel {
                style::ACCENT
            } else {
                style::SUBTEXT1
            };
            let name_color = if is_sel { style::TEXT } else { style::SUBTEXT0 };
            let idx = text(format!("{:>2}", i + 1))
                .size(11.0)
                .color(idx_color)
                .font(Font::MONOSPACE)
                .width(Length::Fixed(20.0));
            let label = text(name.clone()).size(13.0).color(name_color);
            let row_item = iced::widget::row![idx, label]
                .spacing(10.0)
                .align_y(iced::alignment::Vertical::Center)
                .width(Fill);
            let btn = button(row_item)
                .style(style::sample_row(is_sel))
                .on_press(Message::SelectSample(i))
                .padding(8.0)
                .width(Fill);
            list = list.push(btn);
        }

        // ── Footer ─────────────────────────────────────────────────────────
        let footer = text(format!("{} samples", self.samples.len()))
            .size(10.0)
            .color(style::SUBTEXT1)
            .font(Font::MONOSPACE);

        let body = Column::new()
            .push(header)
            .push(rule::horizontal(1.0).style(style::divider))
            .push(section)
            .push(scrollable(list).width(Fill).height(Fill))
            .push(rule::horizontal(1.0).style(style::divider))
            .push(footer)
            .spacing(12.0)
            .width(Fill)
            .height(Fill);

        container(body)
            .style(style::sidebar)
            .width(Length::Fixed(248.0))
            .height(Fill)
            .padding(16.0)
            .into()
    }

    /// The main pane — a breadcrumb top bar (Preview / <sample> · index chip)
    /// over the rendered preview surface.
    fn render_main(&self) -> Element<'_, Message> {
        let (sel, count) = (self.selected_sample, self.samples.len());
        let name = self
            .samples
            .get(sel)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();

        let crumb = text("Preview")
            .size(12.0)
            .color(style::SUBTEXT1)
            .font(Font::MONOSPACE);
        let sep = text("›").size(12.0).color(style::SUBTEXT1);
        let title = text(name).size(14.0).color(style::TEXT);
        let chip = container(
            text(format!("{} / {count}", sel + 1))
                .size(11.0)
                .color(style::ACCENT)
                .font(Font::MONOSPACE),
        )
        .style(style::index_pill)
        .padding([3.0, 8.0]);

        let bar = iced::widget::row![
            crumb,
            sep,
            title,
            iced::widget::Space::new().width(Fill),
            chip,
        ]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(8.0)
        .width(Fill);
        let top_bar = container(bar)
            .style(style::top_bar)
            .padding([14.0, 20.0])
            .width(Fill);

        let preview = self.render_preview();

        Column::new()
            .push(top_bar)
            .push(rule::horizontal(1.0).style(style::divider))
            .push(preview)
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// The rendered surface — a scrollable walk of the `root` component tree on
    /// the base surface fill.
    fn render_preview(&self) -> Element<'_, Message> {
        let tree = self.render_tree("root");
        container(scrollable(tree))
            .style(style::surface)
            .width(Fill)
            .height(Fill)
            .padding(24.0)
            .into()
    }

    /// Walk a component subtree into an [`Element`] tree. Returns a muted
    /// placeholder when the surface / root is missing.
    fn render_tree(&self, root_id: &str) -> Element<'_, Message> {
        let Some(surface) = self.processor.model.surfaces().next() else {
            return text("No surface loaded.").color(style::SUBTEXT1).into();
        };
        if !surface.components.borrow().contains(root_id) {
            return text("No root component").color(style::SUBTEXT1).into();
        }

        let data_model = surface.data_model.borrow();
        let components = surface.components.borrow();
        let focused_id = self.focus.focused_id().map(str::to_string);
        render_node(
            root_id,
            &surface.id,
            "",
            &data_model,
            &components,
            &self.functions,
            focused_id.as_deref(),
            &self.image_cache,
            &self.local_tabs,
        )
    }

    /// One open Modal's `content` subtree in a centered elevated panel with a
    /// title bar + close button, layered over a dimmed click-to-dismiss scrim.
    /// Built as an inner [`Stack`] so the scrim catches clicks that miss the
    /// panel while the panel stays interactive.
    fn render_modal_overlay(&self, modal_id: &str) -> Option<Element<'_, Message>> {
        let surface = self.processor.model.surfaces().next()?;

        // Resolve the modal's content id + optional title in one borrow.
        let (content_id, title): (Option<String>, String) = {
            let components = surface.components.borrow();
            let Some(m) = components.get(modal_id) else {
                return None;
            };
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
            render_node(
                &content_id,
                &surface.id,
                "",
                &data_model,
                &components,
                &self.functions,
                focused_id.as_deref(),
                &self.image_cache,
                &self.local_tabs,
            )
        };

        // ── Panel chrome: title row + divider + content ────────────────────
        let close = button(text("✕").size(13.0).color(style::SUBTEXT0))
            .style(style::borderless)
            .on_press(Message::ModalClose {
                modal_id: modal_id.to_string(),
            })
            .padding(4.0);
        let title_row = iced::widget::row![
            text(title).size(14.0).color(style::TEXT),
            iced::widget::Space::new().width(Fill),
            close,
        ]
        .align_y(iced::alignment::Vertical::Center)
        .width(Fill);
        let panel_body = Column::new()
            .push(title_row)
            .push(rule::horizontal(1.0).style(style::divider))
            .push(content_tree)
            .spacing(14.0)
            .width(Fill);

        let panel = container(panel_body)
            .style(style::modal_panel)
            .padding(24.0)
            .width(Length::Fixed(480.0))
            .max_width(560.0);

        // Center the panel; the scrim button behind it fills the viewport and
        // dismisses the modal when the click lands outside the panel.
        let scrim = button(iced::widget::Space::new().width(Fill).height(Fill))
            .style(style::scrim)
            .on_press(Message::ModalClose {
                modal_id: modal_id.to_string(),
            })
            .width(Fill)
            .height(Fill);
        let centered = container(panel)
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill);

        Some(Stack::new().push(scrim).push(centered).into())
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

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Download a remote image over HTTP and decode it into an Iced
/// [`image::Handle`]. Blocking — run off the UI thread (here, inside the
/// `Task::perform` future in [`fetch_image_task`]).
///
/// A 10 s timeout keeps a slow/dead host from pinning the calling thread; any
/// failure (network or read) returns `None` so the caller can record it as a
/// failed attempt and keep the placeholder.
fn fetch_handle(url: &str) -> Option<image::Handle> {
    // ureq 3: timeout moved from a per-request method to `Agent` config.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut resp = agent.get(url).call().ok()?;
    let bytes = resp.body_mut().read_to_vec().ok()?;
    // `Handle::from_bytes` does no decoding up front — the renderer decodes
    // lazily — so an undecodable payload surfaces later as a blank render,
    // not a panic.
    Some(image::Handle::from_bytes(bytes))
}

/// One remote-image fetch task: download the bytes over HTTP (blocking, on the
/// `thread-pool` executor) and decode them into an Iced [`image::Handle`].
///
/// Iced has no built-in URL image loader — its `image` widget only takes a
/// `Handle` built from a local path or in-memory bytes — so remote `Image`
/// URLs must be fetched and decoded here, then cached by [`IcedApp`]. Any
/// failure becomes [`Message::ImageLoadFailed`] so the placeholder stays and
/// the URL isn't refetched.
fn fetch_image_task(url: String) -> Task<Message> {
    let url_for_msg = url.clone();
    Task::perform(
        async move {
            // Blocking HTTP inside an async future runs on one of the
            // thread-pool executor's threads — acceptable for a gallery
            // fetching a handful of images once per sample.
            fetch_handle(&url)
        },
        move |maybe_handle| match maybe_handle {
            Some(handle) => Message::ImageLoaded {
                url: url_for_msg,
                handle,
            },
            None => Message::ImageLoadFailed { url: url_for_msg },
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fetches a *real* remote image end-to-end (ureq GET → bytes →
    /// `Handle::from_bytes`), proving the Iced backend's remote-image path
    /// works against the exact unsplash URLs the gallery samples use. Ignored
    /// by default so offline / CI runs don't need network; run with
    /// `cargo test -p a2ui-iced --features backend -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn fetch_handle_downloads_real_image() {
        let url = "https://images.unsplash.com/photo-1494790108377-be9c29b29330?w=40&h=40&fit=crop";
        let handle = fetch_handle(url).expect("image should download + decode");
        // A Handle::Bytes carries the raw encoded payload; the renderer decodes
        // it lazily, so we only assert we got *some* bytes back.
        let bytes = match handle {
            image::Handle::Bytes(_, b) => b,
            _ => panic!("expected a Bytes handle"),
        };
        assert!(!bytes.is_empty(), "decoded image bytes should be non-empty");
        // JPEG magic bytes — the unsplash asset is a JPEG.
        assert_eq!(&bytes[..2], &[0xFF, 0xD8], "expected JPEG SOI marker");
        println!("fetched {} bytes for {url}", bytes.len());
    }
}
