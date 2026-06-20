//! Runtime host — owns the Slint window, the message processor, and the bridge
//! between UI events and the framework-agnostic interaction layer in `a2ui_base`.
//!
//! [`SurfaceHost`] is the Slint counterpart of the tui gallery's `GalleryApp`:
//! it holds the [`MessageProcessor`] state, renders the first surface into the
//! `Surface` window via [`live_tree`], and routes control changes back into the
//! data model.
//!
//! Two write-back paths, mirroring the iced/egui backends:
//!
//! - **Button** presses go through the shared core pipeline
//!   ([`dispatch_event`] + [`apply_event_result`]) because a Button's action may
//!   fire a server event / function call, not just a data update.
//! - **All other interactive controls** (TextField, Slider, CheckBox,
//!   ChoicePicker, Tabs, DateTimeInput) write their new value **directly** to the
//!   data model at the control's bound JSON-pointer path. The shared core
//!   `dispatch_event` only models key-driven Button/CheckBox/Slider/TextField and
//!   has no path for pointer-driven native widgets, so — exactly like iced/egui —
//!   we resolve the binding path ourselves and `data_model.set(path, value)`.
//!
//! Shared state lives behind `Rc` so the Slint `Events.*` callbacks (set once at
//! construction) can reach back into the processor. Slint runs on a single
//! thread, so `Rc`/`RefCell` (not `Arc`/`Mutex`) are sufficient.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use serde_json::Value;
use slint::ComponentHandle;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::components::dispatch_event;
use a2ui_base::event::{InputEvent, InputKey};
use a2ui_base::focus::FocusManager;
use a2ui_base::interaction::apply_event_result;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::protocol::common_types::{
    DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList,
};
use a2ui_base::protocol::server_to_client::A2uiMessage;

use crate::live_tree::{build_nodes, build_overlay_nodes, read_options};
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
    /// Decoded raster cache for `Image` nodes (url → `slint::Image`). Local file
    /// paths are decoded up front on sample load; remote/data URLs stay absent
    /// (those images render as placeholders). Cleared on sample switch.
    image_cache: RefCell<HashMap<String, slint::Image>>,
}

impl SurfaceHost {
    /// Create a host: register `catalogs` with a fresh processor, create the
    /// window, wire the control callbacks, and render the initial frame.
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
            image_cache: RefCell::new(HashMap::new()),
        });

        // Button press → core dispatch (the only control routed through the
        // shared pipeline, since its action may fire a server event/function).
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_activate(move |node_id| s.handle_activate(node_id.as_str()));
        }
        // TextField / DateTimeInput text edit → direct data-model write.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_text_edited(move |id, text| {
                    s.handle_text_edited(id.as_str(), text.as_str());
                });
        }
        // Slider drag → direct data-model write.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_slider_changed(move |id, value| {
                    s.handle_slider_changed(id.as_str(), value);
                });
        }
        // CheckBox toggle → direct data-model write.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_check_toggled(move |id, checked| {
                    s.handle_check_toggled(id.as_str(), checked);
                });
        }
        // ChoicePicker single-select pick → resolve label to value, write json!([value]).
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_choice_selected(move |id, label| {
                    s.handle_choice_selected(id.as_str(), label.as_str());
                });
        }
        // ChoicePicker multi-select toggle → recompute the selection array.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_choice_toggled(move |id, index, checked| {
                    s.handle_choice_toggled(id.as_str(), index, checked);
                });
        }
        // Tabs header click → write the active index.
        {
            let s = Rc::clone(&state);
            state
                .surface
                .global::<Events>()
                .on_tab_selected(move |id, index| {
                    s.handle_tab_selected(id.as_str(), index);
                });
        }
        // Gallery sidebar click → load the selected sample.
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
            .map(|(name, _)| SampleEntry {
                name: name.as_str().into(),
            })
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
        self.state.refresh_image_cache();
        self.state.redraw();
    }

    /// Rebuild the focus list from the current component tree.
    pub fn rebuild_focus(&self) {
        let proc = self.state.processor.borrow();
        if let Some(surface) = proc.model.surfaces().next() {
            let components = surface.components.borrow();
            self.state
                .focus
                .borrow_mut()
                .rebuild_from_components(&components);
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
        let image_cache = self.image_cache.borrow();
        let nodes = build_nodes(
            surface,
            &self.functions,
            focused.as_deref(),
            &open_modals,
            &image_cache,
        );
        self.surface.set_nodes(to_node_model(nodes));
        let overlay_nodes = build_overlay_nodes(
            surface,
            &self.functions,
            focused.as_deref(),
            &open_modals,
            &image_cache,
        );
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
                &InputEvent::KeyPress {
                    key: InputKey::Enter,
                },
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

    // -- direct data-model write-back handlers (TextField/Slider/CheckBox/...) --

    /// TextField / DateTimeInput: write the edited text to the bound `value` path.
    fn handle_text_edited(&self, node_id: &str, text: &str) {
        if let Some(path) = self.string_binding_path(node_id, "value") {
            self.apply_data_update(&path, Value::String(text.to_string()));
        }
    }

    /// Slider: write the dragged value to the bound `value` path (clamped by the
    /// Slint widget to [min, max]).
    fn handle_slider_changed(&self, node_id: &str, value: f32) {
        if let Some(path) = self.number_binding_path(node_id, "value") {
            self.apply_data_update(&path, serde_json::json!(value as f64));
        }
    }

    /// CheckBox: write the toggled boolean to the bound `value` path.
    fn handle_check_toggled(&self, node_id: &str, checked: bool) {
        if let Some(path) = self.bool_binding_path(node_id, "value") {
            self.apply_data_update(&path, serde_json::json!(checked));
        }
    }

    /// ChoicePicker single-select: the ComboBox fires `selected` with the picked
    /// label; resolve it to its option value and write `json!([value])`.
    fn handle_choice_selected(&self, node_id: &str, label: &str) {
        let outcome = self.with_ctx(node_id, |model, ctx| {
            let options = read_options(model);
            let value = options
                .iter()
                .find(|(lbl, _)| lbl == label)
                .map(|(_, v)| v.clone())?;
            let path = string_list_binding_path(model, ctx)?;
            Some((path, value))
        });
        if let Some((path, value)) = outcome.flatten() {
            self.apply_data_update(&path, serde_json::json!([value]));
        }
    }

    /// ChoicePicker multi-select: toggle the option at `index` (now `checked`)
    /// in the selection array and write the whole array back.
    fn handle_choice_toggled(&self, node_id: &str, index: i32, checked: bool) {
        if index < 0 {
            return;
        }
        let outcome = self.with_ctx(node_id, |model, ctx| {
            let options = read_options(model);
            let opt_value = options.get(index as usize).map(|(_, v)| v.clone())?;
            let path = string_list_binding_path(model, ctx)?;
            let mut current = current_selection(model, ctx);
            if checked {
                if !current.contains(&opt_value) {
                    current.push(opt_value);
                }
            } else if let Some(pos) = current.iter().position(|v| v == &opt_value) {
                current.remove(pos);
            }
            Some((path, current))
        });
        if let Some((path, values)) = outcome.flatten() {
            let arr: Vec<Value> = values.into_iter().map(Value::String).collect();
            self.apply_data_update(&path, Value::Array(arr));
        }
    }

    /// Tabs: write the clicked tab index to the bound `activeTab` path.
    fn handle_tab_selected(&self, node_id: &str, index: i32) {
        if let Some(path) = self.number_binding_path(node_id, "activeTab") {
            self.apply_data_update(&path, serde_json::json!(index));
        }
    }

    /// Write `value` at `path` in the surface's data model, then redraw.
    fn apply_data_update(&self, path: &str, value: Value) {
        if path.is_empty() {
            return;
        }
        {
            let mut proc = self.processor.borrow_mut();
            if let Some(surface) = proc.model.surfaces_mut().next() {
                surface.data_model.borrow_mut().set(path, value);
            }
        }
        self.redraw();
    }

    /// Borrow the surface, build a [`ComponentContext`] for `node_id`, and run
    /// `f` with the model + context. Returns `None` if the node doesn't exist.
    /// All borrows are dropped before `f`'s result is used.
    fn with_ctx<R>(
        &self,
        node_id: &str,
        f: impl FnOnce(&ComponentModel, &ComponentContext) -> R,
    ) -> Option<R> {
        let proc = self.processor.borrow();
        let surface = proc.model.surfaces().next()?;
        let components = surface.components.borrow();
        let model = components.get(node_id)?;
        let data_model = surface.data_model.borrow();
        let ctx = ComponentContext::new(
            node_id.to_string(),
            surface.id.clone(),
            &data_model,
            &components,
            &self.functions,
            "",
            Some(node_id.to_string()),
        );
        Some(f(model, &ctx))
    }

    /// Resolve the absolute write-back path of a component's string binding.
    fn string_binding_path(&self, node_id: &str, prop: &str) -> Option<String> {
        self.with_ctx(node_id, |model, ctx| {
            match model.get_property::<DynamicString>(prop)? {
                DynamicString::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
                _ => None,
            }
        })
        .flatten()
    }

    /// Resolve the absolute write-back path of a component's number binding.
    fn number_binding_path(&self, node_id: &str, prop: &str) -> Option<String> {
        self.with_ctx(node_id, |model, ctx| {
            match model.get_property::<DynamicNumber>(prop)? {
                DynamicNumber::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
                _ => None,
            }
        })
        .flatten()
    }

    /// Resolve the absolute write-back path of a component's boolean binding.
    fn bool_binding_path(&self, node_id: &str, prop: &str) -> Option<String> {
        self.with_ctx(node_id, |model, ctx| {
            match model.get_property::<DynamicBoolean>(prop)? {
                DynamicBoolean::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
                _ => None,
            }
        })
        .flatten()
    }

    /// Scan the surface's `Image` components and decode their urls into the
    /// cache: local paths / `file://` read directly, `http(s)` urls fetched
    /// with `ureq`, `data:` / undecodable urls skipped (render as placeholders).
    /// Called on sample load + message processing.
    fn refresh_image_cache(&self) {
        let mut cache = self.image_cache.borrow_mut();
        cache.clear();
        let proc = self.processor.borrow();
        let Some(surface) = proc.model.surfaces().next() else {
            return;
        };
        let components = surface.components.borrow();
        let data_model = surface.data_model.borrow();
        for (id, m) in components.all() {
            if m.component_type != "Image" {
                continue;
            }
            let Some(ds) = m.get_property::<DynamicString>("url") else {
                continue;
            };
            let ctx = ComponentContext::new(
                id.clone(),
                surface.id.clone(),
                &data_model,
                &components,
                &self.functions,
                "",
                Some(id.clone()),
            );
            let url = ctx.data_context.resolve_dynamic_string(&ds);
            if url.is_empty() {
                continue;
            }
            if let Some(img) = decode_image(&url) {
                cache.insert(url, img);
            }
        }
    }

    /// Load sample `idx`: reset the processor (keeping catalogs), replay its
    /// messages, refresh focus + image cache, highlight the row, and redraw.
    /// No-op if the index is out of range.
    fn select(&self, idx: usize) {
        let messages = self.samples.borrow().get(idx).map(|(_, msgs)| msgs.clone());
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
        self.refresh_image_cache();
        self.surface.set_selected_sample(idx as i32);
        self.redraw();
    }
}

/// Resolve a ChoicePicker's bound `value` (DynamicStringList) write-back path.
/// Returns `None` when the value is not a data binding.
fn string_list_binding_path(model: &ComponentModel, ctx: &ComponentContext) -> Option<String> {
    match model.get_property::<DynamicStringList>("value")? {
        DynamicStringList::Binding(b) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    }
}

/// Read a ChoicePicker's current selection as a `Vec<String>` (array or single
/// string in the data model), mirroring the TUI/iced references.
fn current_selection(model: &ComponentModel, ctx: &ComponentContext) -> Vec<String> {
    match model.get_property::<DynamicStringList>("value") {
        Some(DynamicStringList::Binding(b)) => match ctx.data_context.get(&b.path) {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        },
        Some(DynamicStringList::Literal(v)) => v,
        _ => Vec::new(),
    }
}

/// Decode an image url into a `slint::Image`.
///
/// - local path / `file://` → read the file;
/// - `http(s)` → fetch the bytes with `ureq` (10 s timeout, mirroring the iced
///   backend's `fetch_handle`);
/// - `data:` / unreadable / unsupported → `None` (renders as a placeholder).
///
/// Fetching runs synchronously on the UI thread (Slint is single-threaded;
/// `Rc`/`RefCell` state can't cross `invoke_from_event_loop`'s `Send` closure),
/// so a sample with many remote images briefly blocks on load. The gallery has
/// few images per sample, so this is acceptable; a future async path would need
/// an `Arc`-backed cache.
fn decode_image(url: &str) -> Option<slint::Image> {
    let bytes = if url.starts_with("http://") || url.starts_with("https://") {
        fetch_bytes(url)?
    } else if url.starts_with("data:") {
        return None;
    } else {
        let path = url.strip_prefix("file://").unwrap_or(url);
        std::fs::read(path).ok()?
    };
    decode_bytes(&bytes)
}

/// Fetch a remote url's bytes over HTTP (blocking; 10 s timeout).
fn fetch_bytes(url: &str) -> Option<Vec<u8>> {
    // ureq 3: timeout moved from a per-request method to `Agent` config.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .into();
    let mut resp = agent.get(url).call().ok()?;
    resp.body_mut().read_to_vec().ok()
}

/// Decode in-memory image bytes (PNG / JPEG / …) into a `slint::Image`.
fn decode_bytes(bytes: &[u8]) -> Option<slint::Image> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let buffer =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h);
    Some(slint::Image::from_rgba8(buffer))
}

/// Wrap a `Vec<LiveNode>` into the `ModelRc` shape the `nodes` property expects.
fn to_node_model(nodes: Vec<LiveNode>) -> slint::ModelRc<LiveNode> {
    slint::ModelRc::new(Rc::new(slint::VecModel::from(nodes)))
}
