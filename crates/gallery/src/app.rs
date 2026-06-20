//! Gallery application — interactive TUI for browsing and rendering A2UI samples.

use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_sci_fi::{Divider, Level, Panel, ScanList, ScanListState, Theme, Value};

use crate::config::{self, GalleryConfig};
use crate::sample_loader::{self, Sample};
use a2ui_base::catalog::Catalog;
use a2ui_base::event::{EventResult, InputEvent, InputKey};
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::DynamicBoolean;
use a2ui_base::protocol::server_to_client::A2uiMessage;
use a2ui_tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui_tui::catalogs::minimal::build_minimal_catalog;
use a2ui_tui::component_impl::ComponentRegistry;
use a2ui_tui::components::image as tui_image;
use a2ui_tui::focus_manager::FocusManager;
use a2ui_tui::surface::SurfaceRenderer;

/// Rows consumed by a [`Panel`] frame: 2 border + 2 one-cell padding.
const PANEL_CHROME_ROWS: usize = 4;
/// Buffer rows each [`ScanList`] item occupies: the text row + its scanline.
const SCANLIST_ROW_STRIDE: usize = 2;

/// Load the sample examples for a catalog (e.g. `"minimal"`, `"basic"`).
///
/// Uses the embedded spec tree ([`sample_loader::load_samples`]) by default so
/// the binary is self-contained. If `A2UI_SPEC_DIR` is set, reads from that
/// on-disk directory instead — a dev override for testing spec changes without
/// recompiling.
fn load_catalog_samples(catalog: &str) -> Vec<Sample> {
    let subpath = format!("v1_0/catalogs/{catalog}/examples");
    if let Ok(root) = std::env::var("A2UI_SPEC_DIR") {
        sample_loader::load_samples_from_dir(&format!("{root}/{subpath}"))
    } else {
        sample_loader::load_samples(&subpath)
    }
}

/// Application mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppMode {
    /// Browsing the sample list (full screen).
    SampleList,
    /// Viewing a rendered sample (split panel).
    Rendered,
}

/// Which panel owns keyboard input while in the split ([`AppMode::Rendered`]) view.
///
/// `List` — ↑/↓ walk the sample list and live-update the right panel.
/// `Render` — keys dispatch to the focused component (stepper, typing, Tab focus).
/// Esc steps `Render → List → SampleList`; Tab/Enter steps back to `Render`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelFocus {
    List,
    Render,
}

/// Snapshot of the data needed to render a single frame.
///
/// This struct is extracted from `GalleryApp` before calling
/// `terminal.draw()` to avoid borrow-checker conflicts between
/// `self.terminal` and the draw closure.
struct FrameData {
    mode: AppMode,
    samples: Vec<(String, String)>, // (name, description)
    selected_sample: usize,
    messages_processed: usize,
    total_messages: usize,
    focused_id: Option<String>,
    /// Which split-view panel is focused (only meaningful in `Rendered` mode).
    panel_focus: PanelFocus,
    /// Active sci-fi theme — drives every chrome color this frame.
    theme: Theme,
    /// Top index of the visible [`ScanList`] window (it does not self-scroll).
    list_scroll: usize,
    /// Animation clock; feeds [`ScanListState::tick`] so the cursor blinks.
    frame_tick: u64,
    /// Active image-protocol name for the status readout (cycled live with `P`).
    image_protocol: &'static str,
}

/// Build the catalog the gallery uses to construct `ComponentContext`s at render
/// and dispatch time.
///
/// The gallery renders samples from both the minimal and basic catalogs, so
/// this catalog must expose the functions from BOTH. The basic catalog is a
/// component superset of minimal (Text/Row/Column/Button/TextField), so only
/// the minimal-only functions (e.g. `capitalize`) are folded in. Without this
/// merge, a minimal sample like "Capitalized Text" silently renders an empty
/// result for its function-bound `Text`, because `capitalize` is absent from
/// the basic catalog — the gallery "had no interaction" for that sample.
pub fn build_gallery_catalog() -> Catalog {
    let mut catalog = build_basic_catalog();
    for (name, func) in build_minimal_catalog().functions {
        catalog.functions.entry(name).or_insert(func);
    }
    catalog
}

/// The gallery application.
pub struct GalleryApp {
    /// Terminal handle.
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
    /// Message processor (owns the surface state).
    processor: MessageProcessor,
    /// Component registry.
    registry: ComponentRegistry,
    /// Catalog.
    catalog: Catalog,
    /// Loaded samples.
    samples: Vec<Sample>,
    /// Index of the currently selected sample.
    selected_sample: usize,
    /// How many messages have been processed for the current sample.
    messages_processed: usize,
    /// Messages for the current sample (cached for replay).
    current_messages: Vec<A2uiMessage>,
    /// Focus manager.
    focus_manager: FocusManager,
    /// Whether the app is still running.
    running: bool,
    /// Current display mode.
    mode: AppMode,
    /// Which split-view panel owns keyboard input (only used in `Rendered` mode).
    panel_focus: PanelFocus,
    /// Active sci-fi theme (cycled live with `t`).
    theme: Theme,
    /// Top index of the visible [`ScanList`] window.
    list_scroll: usize,
    /// Per-frame animation clock for blinking cursors.
    frame_tick: u64,
    /// Persisted user config (image-protocol choice, …). Mutated by the `P`
    /// key and saved to `<config_dir>/a2ui/config.toml`.
    config: GalleryConfig,
}

impl GalleryApp {
    /// Create and initialize the gallery application with the default embedded
    /// spec samples (both minimal and basic catalogs).
    pub fn new() -> io::Result<Self> {
        // Load samples from both minimal and basic directories.
        let samples = {
            let mut s = load_catalog_samples("minimal");
            s.extend(load_catalog_samples("basic"));
            s
        };
        Self::with_samples(samples)
    }

    /// Create the gallery application driven by an explicit sample set.
    ///
    /// Both the minimal and basic catalogs are still registered with the
    /// processor (so a scenario referencing either catalog resolves correctly);
    /// only the browsable sample list is replaced. Used by the `json_gallery`
    /// example to browse ad-hoc `a2ui-json/*.json` scenarios instead of the
    /// embedded spec tree.
    pub fn with_samples(samples: Vec<Sample>) -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stderr());
        let terminal = Terminal::new(backend)?;

        let basic_catalog = build_basic_catalog();
        let minimal_catalog = build_minimal_catalog();
        let registry = build_basic_registry();
        let processor = MessageProcessor::new(vec![basic_catalog, minimal_catalog]);

        // Catalog used to build ComponentContexts at render + dispatch time.
        let catalog = build_gallery_catalog();

        // Load the persisted config and apply the image-protocol choice before
        // the first render (the terminal probe itself stays lazy).
        let config = config::load();
        if let Some(name) = &config.image_protocol {
            tui_image::set_image_protocol(tui_image::ImageProtocol::from_name(name));
        }

        Ok(Self {
            terminal,
            processor,
            registry,
            catalog,
            samples,
            selected_sample: 0,
            messages_processed: 0,
            current_messages: Vec::new(),
            focus_manager: FocusManager::new(),
            running: true,
            mode: AppMode::SampleList,
            panel_focus: PanelFocus::Render,
            // Green phosphor is the default look; `t` cycles the other 7.
            theme: Theme::Fallout,
            list_scroll: 0,
            frame_tick: 0,
            config,
        })
    }

    /// Run the main event loop.
    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(io::stderr(), EnterAlternateScreen)?;
        self.terminal.clear()?;

        while self.running {
            // ScanList doesn't scroll — it paints item i at area.y + i*2 — so the
            // gallery owns the window offset, recomputed each frame from the live
            // terminal size and the active layout.
            let area = self.terminal.size()?;
            let cap = match self.mode {
                // Full-screen list: the Panel fills the frame.
                AppMode::SampleList => {
                    (area.height as usize).saturating_sub(PANEL_CHROME_ROWS) / SCANLIST_ROW_STRIDE
                }
                // Split view: the left panel sits in the 95% column.
                AppMode::Rendered => {
                    let panel_h = (area.height as usize * 95 / 100).max(1);
                    panel_h.saturating_sub(PANEL_CHROME_ROWS) / SCANLIST_ROW_STRIDE
                }
            };
            self.ensure_list_visible(cap);
            self.frame_tick = self.frame_tick.wrapping_add(1);

            // Extract frame data before drawing to avoid borrow conflicts.
            let fd = self.snapshot_frame_data();

            let registry = &self.registry;
            let catalog = &self.catalog;

            // We need a reference to the surface for rendering.
            // Safety: we only read from processor.model during the draw.
            let surface_ref = self.processor.model.surfaces().next();

            self.terminal.draw(|frame| match fd.mode {
                AppMode::SampleList => render_sample_list(frame, &fd),
                AppMode::Rendered => render_split_view(
                    frame,
                    &fd,
                    surface_ref,
                    registry,
                    catalog,
                    fd.focused_id.as_deref(),
                ),
            })?;

            if event::poll(std::time::Duration::from_millis(100))? {
                let ev = event::read()?;
                self.handle_event(ev);
            }
        }

        // Restore terminal.
        disable_raw_mode()?;
        execute!(io::stderr(), LeaveAlternateScreen)?;

        Ok(())
    }

    /// Collect a snapshot of data needed for rendering.
    fn snapshot_frame_data(&self) -> FrameData {
        let samples: Vec<(String, String)> = self
            .samples
            .iter()
            .map(|s| (s.name.clone(), s.description.clone()))
            .collect();

        FrameData {
            mode: self.mode,
            samples,
            selected_sample: self.selected_sample,
            messages_processed: self.messages_processed,
            total_messages: self.current_messages.len(),
            focused_id: self.focus_manager.focused_id().map(|s| s.to_string()),
            panel_focus: self.panel_focus,
            theme: self.theme,
            list_scroll: self.list_scroll,
            frame_tick: self.frame_tick,
            image_protocol: tui_image::detected_protocol(),
        }
    }

    // -----------------------------------------------------------------------
    // Event handling
    // -----------------------------------------------------------------------

    /// Handle a single terminal event.
    fn handle_event(&mut self, ev: Event) {
        if let Event::Key(key) = ev {
            // Only process key press events (ignore release).
            if key.kind != KeyEventKind::Press {
                return;
            }
            match self.mode {
                AppMode::SampleList => self.handle_sample_list_key(key.code),
                AppMode::Rendered => self.handle_rendered_key(key.code),
            }
        }
    }

    fn handle_sample_list_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Char('t') => self.cycle_theme(),
            KeyCode::Char('p') | KeyCode::Char('P') => self.cycle_image_protocol(),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_sample > 0 {
                    self.selected_sample -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.samples.is_empty() && self.selected_sample < self.samples.len() - 1 {
                    self.selected_sample += 1;
                }
            }
            KeyCode::Enter => {
                self.select_sample(self.selected_sample);
            }
            _ => {}
        }
    }

    fn handle_rendered_key(&mut self, code: KeyCode) {
        match self.panel_focus {
            PanelFocus::List => self.handle_list_focus_key(code),
            PanelFocus::Render => self.handle_surface_focus_key(code),
        }
    }

    /// Keys while the sample list owns focus (split view): walk samples with
    /// live right-panel update, then hand focus to the surface to interact.
    fn handle_list_focus_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => self.running = false,
            // Esc steps back: list focus → full-screen sample browser.
            KeyCode::Esc => self.mode = AppMode::SampleList,
            // `t` cycles the theme; only bound here (not in surface focus) so a
            // focused A2UI TextInput still receives a typed 't'.
            KeyCode::Char('t') => self.cycle_theme(),
            // `P` cycles the image protocol (persisted); also browsing-only so a
            // typed 'p' reaches a focused TextInput.
            KeyCode::Char('p') | KeyCode::Char('P') => self.cycle_image_protocol(),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_sample > 0 {
                    self.load_sample_full(self.selected_sample - 1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.samples.is_empty() && self.selected_sample < self.samples.len() - 1 {
                    self.load_sample_full(self.selected_sample + 1);
                }
            }
            // Enter / Tab / → : move focus to the rendered surface.
            KeyCode::Enter | KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.panel_focus = PanelFocus::Render;
            }
            _ => {}
        }
    }

    /// Keys while the rendered surface owns focus (split view): stepper,
    /// component focus cycling, and dispatch to the focused component.
    fn handle_surface_focus_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => self.running = false,
            // Esc steps back: surface focus → list focus (so ↑/↓ walk samples).
            // A second Esc (handled in list focus) returns to the sample browser.
            KeyCode::Esc => self.panel_focus = PanelFocus::List,
            KeyCode::Char('n') => {
                // Process next message (stepper).
                self.process_next_message();
                self.rebuild_focus();
            }
            KeyCode::Char('a') => {
                // Process all remaining messages.
                self.process_remaining_messages();
                self.rebuild_focus();
            }
            KeyCode::Char('r') => {
                // Reset and replay all messages.
                self.replay_current_sample();
            }
            KeyCode::Tab => {
                self.focus_manager.focus_next();
            }
            KeyCode::BackTab => {
                self.focus_manager.focus_prev();
            }
            _ => {
                self.dispatch_event_to_focused(code);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Event dispatch
    // -----------------------------------------------------------------------

    /// Dispatch a keyboard event to the focused component.
    fn dispatch_event_to_focused(&mut self, code: KeyCode) {
        // Map KeyCode to InputKey
        let input_key = match code {
            KeyCode::Enter => InputKey::Enter,
            KeyCode::Tab => InputKey::Tab,
            KeyCode::BackTab => InputKey::BackTab,
            KeyCode::Up => InputKey::Up,
            KeyCode::Down => InputKey::Down,
            KeyCode::Left => InputKey::Left,
            KeyCode::Right => InputKey::Right,
            KeyCode::Backspace => InputKey::Backspace,
            KeyCode::Delete => InputKey::Delete,
            KeyCode::Esc => InputKey::Escape,
            KeyCode::Char(' ') => InputKey::Space,
            KeyCode::Char(c) => InputKey::Char(c),
            _ => return,
        };

        let event = InputEvent::KeyPress { key: input_key };

        // Get focused component ID
        let focused_id = match self.focus_manager.focused_id() {
            Some(id) => id.to_string(),
            None => return,
        };

        // Get surface and component info
        let surface = match self.processor.model.surfaces().next() {
            Some(s) => s,
            None => return,
        };

        let (comp_type, surface_id) = {
            let components = surface.components.borrow();
            let comp_model = match components.get(&focused_id) {
                Some(m) => m,
                None => return,
            };
            (comp_model.component_type.clone(), surface.id.clone())
        };

        // Find the TuiComponent in the registry
        let tui_comp = match self.registry.get(&comp_type) {
            Some(c) => c,
            None => return,
        };

        // Build a ComponentContext
        let data_model = surface.data_model.borrow();
        let components = surface.components.borrow();
        let catalog_functions = &self.catalog.functions;

        let ctx = ComponentContext::new(
            focused_id.clone(),
            surface_id,
            &data_model,
            &components,
            catalog_functions,
            "",
            Some(focused_id.clone()),
        );

        // Dispatch event
        let result = tui_comp.handle_event(&ctx, &event);

        // Process result — must drop borrows before mutating
        drop(components);
        drop(data_model);
        if let Some(result) = result {
            self.process_event_result(result);
        }

        // Modal open/close is handled locally: A2UI routes it through a server
        // event the gallery can't answer, so when the focused node is some
        // Modal's trigger we toggle that Modal's `isOpen` directly. The tui
        // ModalComponent reads `isOpen` and swaps trigger↔content on the next
        // render; pressing Enter again (focus stays on the trigger) closes it.
        self.apply_modal_interaction(&focused_id);
    }

    /// Toggle a Modal's open state when its trigger is activated.
    ///
    /// `node_id` is the just-activated component. If it is some Modal's
    /// `trigger`, that Modal's `isOpen` is flipped on its component model
    /// (written as a `DynamicBoolean::Literal` so the value round-trips through
    /// the tui render path without coupling to the serde tag layout by hand).
    /// No-op for nodes that are no Modal's trigger.
    fn apply_modal_interaction(&mut self, node_id: &str) {
        let surface = match self.processor.model.surfaces().next() {
            Some(s) => s,
            None => return,
        };

        // Find the Modal whose trigger is `node_id`.
        let modal_id = {
            let components = surface.components.borrow();
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
        let Some(modal_id) = modal_id else { return };

        // Toggle `isOpen`. Only a previously-written `Literal(true)` counts as
        // open; anything else (no property, a data binding, …) is the closed
        // baseline, so the first activation always opens.
        let mut components = surface.components.borrow_mut();
        if let Some(m) = components.get_mut(&modal_id) {
            let open = matches!(
                m.get_property::<DynamicBoolean>("isOpen"),
                Some(DynamicBoolean::Literal(true))
            );
            let next = serde_json::to_value(DynamicBoolean::Literal(!open))
                .unwrap_or_else(|_| serde_json::json!({"Literal": false}));
            m.properties.insert("isOpen".into(), next);
        }
    }

    /// Process an EventResult from a component.
    fn process_event_result(&mut self, result: EventResult) {
        // Deconstruct the result to separate data mutations from action registration,
        // avoiding borrow conflicts with self.processor.
        match result {
            EventResult::Action {
                want_response,
                response_path,
                ..
            } => {
                // Note: we intentionally do NOT eprintln here — the TUI renders
                // into stderr, so any write would corrupt the display.

                if want_response {
                    // Get the surface ID first, then register the action separately.
                    let surface_id = self.processor.model.surfaces().next().map(|s| s.id.clone());
                    if let Some(sid) = surface_id {
                        let action_id = uuid::Uuid::new_v4().to_string();
                        let _ = self
                            .processor
                            .register_action(&sid, &action_id, response_path);
                    }
                }
            }
            EventResult::DataUpdate { path, value } => {
                if let Some(surface) = self.processor.model.surfaces_mut().next() {
                    surface.data_model.borrow_mut().set(&path, value);
                }
            }
            EventResult::Toggle { path } => {
                if let Some(surface) = self.processor.model.surfaces_mut().next() {
                    let current = surface
                        .data_model
                        .borrow()
                        .get(&path)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    surface
                        .data_model
                        .borrow_mut()
                        .set(&path, serde_json::json!(!current));
                }
            }
            EventResult::Consumed => {}
        }
    }

    // -----------------------------------------------------------------------
    // Sample management
    // -----------------------------------------------------------------------

    /// Select a sample, switch to rendered mode, and load it at its **first step**.
    ///
    /// Only the initial message is processed, so the sample's progressive
    /// evolution can be revealed with the `n` stepper; press `a` to jump to the
    /// final state.
    fn select_sample(&mut self, index: usize) {
        if index >= self.samples.len() {
            return;
        }
        self.load_sample(index);
        self.panel_focus = PanelFocus::Render;
        self.mode = AppMode::Rendered;
    }

    /// Load `index`'s messages at the **first step** (only the initial message
    /// processed) and reset interaction state.
    ///
    /// Entering a sample at the start — rather than processing every message at
    /// once — is what lets progressive samples like "Incremental Dashboard"
    /// reveal their step-by-step evolution via the `n` stepper. Use
    /// [`Self::load_sample_full`] when a complete preview is wanted (e.g. while
    /// browsing the list), and `a` to jump to the final state once open.
    fn load_sample(&mut self, index: usize) {
        self.load_sample_inner(index, false);
    }

    /// Like [`Self::load_sample`], but processes **all** messages — used for the
    /// split-view browsing preview so the right panel shows the finished sample.
    fn load_sample_full(&mut self, index: usize) {
        self.load_sample_inner(index, true);
    }

    /// Shared core of [`Self::load_sample`] / [`Self::load_sample_full`]. Does
    /// NOT touch `mode` or `panel_focus` — callers decide those.
    fn load_sample_inner(&mut self, index: usize, process_all: bool) {
        if index >= self.samples.len() {
            return;
        }

        // Reset processor state for the new sample, keeping catalogs registered
        // (resetting with empty catalogs would flag every component as unknown
        // and pollute the TUI with warnings).
        self.processor.reset();

        self.current_messages = self.samples[index].messages.clone();
        self.messages_processed = 0;
        self.focus_manager.reset();
        self.selected_sample = index;

        if process_all {
            self.process_remaining_messages();
        } else {
            // Process only the first message (the `createSurface`) so the
            // stepper begins at step 1 of N.
            self.process_next_message();
        }
        self.rebuild_focus();
    }

    /// Process exactly one unprocessed message — the stepper's single step.
    /// Returns `true` if a message was processed.
    fn process_next_message(&mut self) -> bool {
        if self.messages_processed < self.current_messages.len() {
            let msg = self.current_messages[self.messages_processed].clone();
            let _ = self.processor.process_message(msg);
            self.messages_processed += 1;
            true
        } else {
            false
        }
    }

    /// Process all remaining unprocessed messages.
    fn process_remaining_messages(&mut self) {
        while self.process_next_message() {}
    }

    /// Reset and replay the current sample from its **first step**.
    fn replay_current_sample(&mut self) {
        let messages = self.current_messages.clone();
        self.processor.reset();
        self.current_messages = messages;
        self.messages_processed = 0;
        self.focus_manager.reset();

        self.process_next_message();
        self.rebuild_focus();
    }

    /// Rebuild the focus list from the first available surface.
    fn rebuild_focus(&mut self) {
        if let Some(surface) = self.processor.model.surfaces().next() {
            let components = surface.components.borrow();
            self.focus_manager.rebuild_from_components(&components);
        }
    }

    // -----------------------------------------------------------------------
    // Theming & list windowing
    // -----------------------------------------------------------------------

    /// Advance to the next sci-fi theme (Cyberpunk → … → Sentinel → Cyberpunk).
    fn cycle_theme(&mut self) {
        self.theme = match self.theme {
            Theme::Cyberpunk => Theme::Fallout,
            Theme::Fallout => Theme::Weyland,
            Theme::Weyland => Theme::DeepSpace,
            Theme::DeepSpace => Theme::Bloodmoon,
            Theme::Bloodmoon => Theme::Nebula,
            Theme::Nebula => Theme::Arctic,
            Theme::Arctic => Theme::Sentinel,
            Theme::Sentinel => Theme::Cyberpunk,
        };
    }

    /// Cycle the image protocol (bound to `P`) and persist the choice to the
    /// config file. Order: Auto → Halfblocks → Kitty → Sixel → iTerm2 → None.
    fn cycle_image_protocol(&mut self) {
        let next = next_image_protocol(tui_image::current_image_protocol());
        tui_image::set_image_protocol(next);
        self.config.image_protocol = Some(next.as_str().to_string());
        if let Err(e) = config::save(&self.config) {
            // The TUI paints stderr, so this only lands after quit — still
            // better than silently dropping the save failure.
            eprintln!("Warning: could not save config: {e}");
        }
    }

    /// Keep `list_scroll` pinned so `selected_sample` stays inside the window.
    ///
    /// `capacity` is how many [`ScanList`] items fit the current panel; the
    /// offset only moves when the selection escapes the window, so short walks
    /// don't jitter the list.
    fn ensure_list_visible(&mut self, capacity: usize) {
        self.list_scroll = compute_list_scroll(
            self.selected_sample,
            self.samples.len(),
            capacity,
            self.list_scroll,
        );
    }
}

// ---------------------------------------------------------------------------
// Free rendering functions (operate on extracted data, not on GalleryApp)
// ---------------------------------------------------------------------------

/// Render the sample list (full screen) inside a themed [`Panel`].
fn render_sample_list(frame: &mut ratatui::Frame, fd: &FrameData) {
    let area = frame.area();
    let panel = Panel::new()
        .title(format!(" A2UI GALLERY // {} ", theme_name(fd.theme)))
        .theme(fd.theme);
    let inner = panel.inner(area);
    frame.render_widget(panel, area);

    let rows = sample_rows(&fd.samples, fd.list_scroll, inner.height, true);
    let mut state = ScanListState {
        selected: fd.selected_sample.saturating_sub(fd.list_scroll),
        tick: fd.frame_tick,
    };
    frame.render_stateful_widget(ScanList::new(rows).theme(fd.theme), inner, &mut state);
}

/// Render the split view: sample list on the left, surface on the right.
fn render_split_view(
    frame: &mut ratatui::Frame,
    fd: &FrameData,
    surface: Option<&a2ui_base::model::surface_model::SurfaceModel>,
    registry: &ComponentRegistry,
    catalog: &Catalog,
    focused_id: Option<&str>,
) {
    let area = frame.area();

    // Main panels (95%) + a 2-row footer (divider + help line).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(95), Constraint::Length(2)])
        .split(area);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);

    // Left: compact sample list.
    render_sample_list_panel(frame, fd, panels[0], fd.panel_focus == PanelFocus::List);

    // Right: rendered surface (content unchanged; only the frame is themed).
    render_surface_panel(
        frame,
        panels[1],
        surface,
        registry,
        catalog,
        focused_id,
        fd.panel_focus == PanelFocus::Render,
        fd.theme,
    );

    // Bottom: themed divider + help line.
    render_help_bar(frame, outer[1], fd);
}

/// Render the sample list in a side panel (compact rows, no description).
fn render_sample_list_panel(frame: &mut ratatui::Frame, fd: &FrameData, area: Rect, focused: bool) {
    let title = if focused {
        " ◄ SAMPLES "
    } else {
        " SAMPLES "
    };
    let panel = Panel::new().title(title).theme(fd.theme);
    let inner = panel.inner(area);
    frame.render_widget(panel, area);

    let rows = sample_rows(&fd.samples, fd.list_scroll, inner.height, false);
    let mut state = ScanListState {
        selected: fd.selected_sample.saturating_sub(fd.list_scroll),
        tick: fd.frame_tick,
    };
    frame.render_stateful_widget(ScanList::new(rows).theme(fd.theme), inner, &mut state);
}

/// Build the windowed, formatted rows for a [`ScanList`].
///
/// [`ScanList`] draws item `i` at `area.y + i*2` and never scrolls, so a visible
/// window is sliced out of `samples` starting at `offset`. `with_desc` includes
/// the `— description` suffix (full-screen browser only).
fn sample_rows(
    samples: &[(String, String)],
    offset: usize,
    area_height: u16,
    with_desc: bool,
) -> Vec<String> {
    let cap = ((area_height as usize) / SCANLIST_ROW_STRIDE).max(1);
    let start = offset.min(samples.len());
    let end = (start + cap).min(samples.len());
    samples[start..end]
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            let idx = start + i + 1;
            if with_desc {
                format!("{idx:>2}. {name} — {desc}")
            } else {
                format!("{idx:>2}. {name}")
            }
        })
        .collect()
}

/// Render the current surface inside a themed [`Panel`].
///
/// A bordered frame is always drawn so the panel's focus state is visible (the
/// `►` title marker when focused); the surface itself is rendered into the
/// inner area via [`SurfaceRenderer`] so it — the A2UI sample being previewed —
/// is never restyled by the gallery chrome.
#[allow(clippy::too_many_arguments)] // gallery panel: frame + area + surface + shared render deps + theme
fn render_surface_panel(
    frame: &mut ratatui::Frame,
    area: Rect,
    surface: Option<&a2ui_base::model::surface_model::SurfaceModel>,
    registry: &ComponentRegistry,
    catalog: &Catalog,
    focused_id: Option<&str>,
    focused: bool,
    theme: Theme,
) {
    let title = if focused {
        " SURFACE ► "
    } else {
        " SURFACE "
    };
    let panel = Panel::new().title(title).theme(theme);
    let inner = panel.inner(area);
    frame.render_widget(panel, area);

    if let Some(surface) = surface {
        let renderer = SurfaceRenderer::new(surface, registry, catalog);
        renderer.render(frame, inner, focused_id);
    } else {
        let palette = theme.palette();
        let paragraph = Paragraph::new("No surface loaded.\nPress 'n' to step through messages.")
            .style(Style::default().fg(palette.muted.color()));
        frame.render_widget(paragraph, inner);
    }
}

/// Render the bottom help bar: a themed [`Divider`] rule above a one-line hint
/// row (key hints on the left, a telemetry [`Value`] readout on the right).
fn render_help_bar(frame: &mut ratatui::Frame, area: Rect, fd: &FrameData) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    frame.render_widget(Divider::new().theme(fd.theme), rows[0]);

    let palette = fd.theme.palette();
    let accent = Style::default().fg(palette.accent.color());
    let muted = Style::default().fg(palette.muted.color());

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(rows[1]);

    frame.render_widget(Paragraph::new(help_line(fd, accent, muted)), cols[0]);
    frame.render_widget(help_readout(fd), cols[1]);
}

/// The contextual one-line key hint for the help bar (active tag in accent,
/// the rest muted). `t:theme` / `P:img` appear only in the browsing contexts.
fn help_line(fd: &FrameData, accent: Style, muted: Style) -> Line<'static> {
    match fd.mode {
        AppMode::SampleList => Line::from(vec![
            Span::styled(" A2UI GALLERY ", accent),
            Span::styled(
                " ↑/k up · ↓/j down · Enter select · t theme · P img · q/Esc quit",
                muted,
            ),
        ]),
        AppMode::Rendered => match fd.panel_focus {
            PanelFocus::List => Line::from(vec![
                Span::styled(" [List ◄] ", accent),
                Span::styled(
                    " ↑/↓ sample · Tab/Enter surface · Esc browser · t theme · P img · q quit",
                    muted,
                ),
            ]),
            PanelFocus::Render => Line::from(vec![
                Span::styled(" [Surface ►] ", accent),
                Span::styled(
                    " n step · a all · r restart · Tab focus · Esc list · q quit",
                    muted,
                ),
            ]),
        },
    }
}

/// The right-hand telemetry readout for the help bar: the live theme name in
/// the browser, or the message stepper `processed/total` (Ok once complete).
/// The active image protocol (cycled with `P`) is always appended.
fn help_readout(fd: &FrameData) -> Value {
    match fd.mode {
        AppMode::SampleList => {
            Value::new(format!("{} · {}", theme_name(fd.theme), fd.image_protocol))
                .label("THEME/IMG")
                .theme(fd.theme)
        }
        AppMode::Rendered => {
            if fd.total_messages == 0 {
                Value::new(fd.image_protocol.to_string())
                    .label("IMG")
                    .theme(fd.theme)
            } else {
                let done = fd.messages_processed >= fd.total_messages;
                Value::new(format!(
                    "{}/{} · {}",
                    fd.messages_processed, fd.total_messages, fd.image_protocol
                ))
                .label("MSG/IMG")
                .state(if done { Level::Ok } else { Level::Warn })
                .theme(fd.theme)
            }
        }
    }
}

/// Next image protocol in the `P`-key cycle:
/// Auto → Halfblocks → Kitty → Sixel → iTerm2 → None → Auto.
fn next_image_protocol(p: tui_image::ImageProtocol) -> tui_image::ImageProtocol {
    use tui_image::ImageProtocol;
    match p {
        ImageProtocol::Auto => ImageProtocol::Halfblocks,
        ImageProtocol::Halfblocks => ImageProtocol::Kitty,
        ImageProtocol::Kitty => ImageProtocol::Sixel,
        ImageProtocol::Sixel => ImageProtocol::Iterm2,
        ImageProtocol::Iterm2 => ImageProtocol::None,
        ImageProtocol::None => ImageProtocol::Auto,
    }
}

/// Pure scroll-offset rule: keep `selected` inside `[offset, offset+cap)`,
/// moving `offset` only when the selection escapes. Shared by
/// [`GalleryApp::ensure_list_visible`] (runtime) and the unit tests.
fn compute_list_scroll(selected: usize, len: usize, cap: usize, prev_offset: usize) -> usize {
    let cap = cap.max(1);
    if len == 0 {
        return 0;
    }
    let mut offset = prev_offset;
    if selected < offset {
        offset = selected;
    } else if selected >= offset + cap {
        offset = selected - cap + 1;
    }
    if offset + cap > len {
        offset = len.saturating_sub(cap);
    }
    offset
}

/// Upcased display name of a theme, for titles and the THEME readout.
fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::Cyberpunk => "CYBERPUNK",
        Theme::Fallout => "FALLOUT",
        Theme::Weyland => "WEYLAND",
        Theme::DeepSpace => "DEEP SPACE",
        Theme::Bloodmoon => "BLOODMOON",
        Theme::Nebula => "NEBULA",
        Theme::Arctic => "ARCTIC",
        Theme::Sentinel => "SENTINEL",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, layout::Rect};

    /// A throwaway [`FrameData`] for render tests: 6 samples, SampleList mode.
    fn fd(selected: usize, scroll: usize, tick: u64) -> FrameData {
        let samples: Vec<(String, String)> = (0..6)
            .map(|i| (format!("sample{i}"), format!("desc{i}")))
            .collect();
        FrameData {
            mode: AppMode::SampleList,
            samples,
            selected_sample: selected,
            messages_processed: 0,
            total_messages: 0,
            focused_id: None,
            panel_focus: PanelFocus::List,
            theme: Theme::Cyberpunk,
            list_scroll: scroll,
            frame_tick: tick,
            image_protocol: "Halfblocks",
        }
    }

    /// Render the full-screen sample list into an offscreen buffer and return it.
    fn draw_sample_list(width: u16, height: u16, data: &FrameData) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_sample_list(f, data)).unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn full_screen_panel_uses_double_border() {
        let buf = draw_sample_list(40, 20, &fd(0, 0, 0));
        assert_eq!(
            buf[(0, 0)].symbol(),
            "╔",
            "Panel top-left must be a double-line corner"
        );
    }

    #[test]
    fn scanlist_cursor_blinks() {
        // The cursor sits at the Panel inner origin (border + 1-cell padding).
        let inner = Panel::new()
            .theme(Theme::Cyberpunk)
            .inner(Rect::new(0, 0, 40, 20));
        let on = draw_sample_list(40, 20, &fd(0, 0, 0));
        assert_eq!(
            on[(inner.x, inner.y)].symbol(),
            "█",
            "cursor visible at tick 0"
        );
        let off = draw_sample_list(40, 20, &fd(0, 0, 15));
        assert_eq!(
            off[(inner.x, inner.y)].symbol(),
            " ",
            "cursor hidden at tick 15"
        );
    }

    #[test]
    fn scroll_rule_tracks_selection() {
        // 6 items, cap 4: selections 0..3 keep offset 0; 4 → 1; 5 → 2.
        assert_eq!(compute_list_scroll(0, 6, 4, 0), 0);
        assert_eq!(compute_list_scroll(3, 6, 4, 0), 0);
        assert_eq!(compute_list_scroll(4, 6, 4, 0), 1);
        assert_eq!(compute_list_scroll(5, 6, 4, 0), 2);
        // Walking back up follows the selection down.
        assert_eq!(compute_list_scroll(1, 6, 4, 2), 1);
        // An empty list never scrolls.
        assert_eq!(compute_list_scroll(99, 0, 4, 5), 0);
    }

    #[test]
    fn scanlist_renders_scrolled_selection() {
        // 6 samples in a 20x12 frame → Panel inner height 8 → 4 visible items.
        let inner = Panel::new()
            .theme(Theme::Cyberpunk)
            .inner(Rect::new(0, 0, 20, 12));
        let cap = (inner.height as usize) / SCANLIST_ROW_STRIDE;
        let selected = 5;
        let scroll = compute_list_scroll(selected, 6, cap, 0);
        assert_eq!(scroll, 2, "item 5 should scroll the window to offset 2");

        let buf = draw_sample_list(20, 12, &fd(selected, scroll, 0));
        let relative = selected - scroll;
        let row_y = inner.y + (relative as u16) * 2;
        assert_eq!(
            buf[(inner.x, row_y)].symbol(),
            "█",
            "the scrolled selection's row must carry the cursor"
        );
    }

    #[test]
    fn help_bar_renders_divider_rule() {
        let backend = TestBackend::new(24, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        let data = fd(0, 0, 0);
        terminal
            .draw(|f| render_help_bar(f, f.area(), &data))
            .unwrap();
        let buf = terminal.backend().buffer();
        for x in 0..24 {
            assert_eq!(
                buf[(x, 0)].symbol(),
                "─",
                "divider row x={x} must be the rule glyph"
            );
        }
    }
}
