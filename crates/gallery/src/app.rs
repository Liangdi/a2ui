//! Gallery application — interactive TUI for browsing and rendering A2UI samples.

use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use a2ui_core::catalog::Catalog;
use a2ui_core::event::{EventResult, InputEvent, InputKey};
use a2ui_core::message_processor::MessageProcessor;
use a2ui_core::model::component_context::ComponentContext;
use a2ui_core::protocol::server_to_client::A2uiMessage;
use crate::sample_loader::{self, Sample};
use a2ui_tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui_tui::catalogs::minimal::build_minimal_catalog;
use a2ui_tui::component_impl::ComponentRegistry;
use a2ui_tui::focus_manager::FocusManager;
use a2ui_tui::surface::SurfaceRenderer;

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
    /// List state for ratatui List widget highlighting.
    list_state: ListState,
}

impl GalleryApp {
    /// Create and initialize the gallery application.
    pub fn new() -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stderr());
        let terminal = Terminal::new(backend)?;

        let basic_catalog = build_basic_catalog();
        let minimal_catalog = build_minimal_catalog();
        let catalog = build_basic_catalog(); // real catalog for ComponentContext building
        let registry = build_basic_registry();
        let processor = MessageProcessor::new(vec![basic_catalog, minimal_catalog]);

        // Load samples from both minimal and basic directories.
        let mut samples = load_catalog_samples("minimal");
        samples.extend(load_catalog_samples("basic"));

        let mut list_state = ListState::default();
        if !samples.is_empty() {
            list_state.select(Some(0));
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
            list_state,
        })
    }

    /// Run the main event loop.
    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(io::stderr(), EnterAlternateScreen)?;
        self.terminal.clear()?;

        while self.running {
            // Extract frame data before drawing to avoid borrow conflicts.
            let fd = self.snapshot_frame_data();

            let registry = &self.registry;
            let catalog = &self.catalog;
            let list_state = &mut self.list_state;

            // We need a reference to the surface for rendering.
            // Safety: we only read from processor.model during the draw.
            let surface_ref = self.processor.model.surfaces().next();

            self.terminal.draw(|frame| {
                match fd.mode {
                    AppMode::SampleList => {
                        render_sample_list(frame, &fd, list_state);
                    }
                    AppMode::Rendered => {
                        render_split_view(
                            frame,
                            &fd,
                            list_state,
                            surface_ref,
                            registry,
                            catalog,
                            fd.focused_id.as_deref(),
                        );
                    }
                }
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
            KeyCode::Char('q') | KeyCode::Esc => {
                self.running = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_sample > 0 {
                    self.selected_sample -= 1;
                    self.list_state.select(Some(self.selected_sample));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.samples.is_empty() && self.selected_sample < self.samples.len() - 1 {
                    self.selected_sample += 1;
                    self.list_state.select(Some(self.selected_sample));
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
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_sample > 0 {
                    self.load_sample(self.selected_sample - 1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.samples.is_empty()
                    && self.selected_sample < self.samples.len() - 1
                {
                    self.load_sample(self.selected_sample + 1);
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
                if self.messages_processed < self.current_messages.len() {
                    let msg = self.current_messages[self.messages_processed].clone();
                    let _ = self.processor.process_message(msg);
                    self.messages_processed += 1;
                    self.rebuild_focus();
                }
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
                    let surface_id = self
                        .processor
                        .model
                        .surfaces()
                        .next()
                        .map(|s| s.id.clone());
                    if let Some(sid) = surface_id {
                        let action_id = uuid::Uuid::new_v4().to_string();
                        let _ = self.processor.register_action(&sid, &action_id, response_path);
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

    /// Select a sample, switch to rendered mode, and process all messages.
    /// Select a sample, switch to rendered mode, and process all messages.
    fn select_sample(&mut self, index: usize) {
        if index >= self.samples.len() {
            return;
        }
        self.load_sample(index);
        self.panel_focus = PanelFocus::Render;
        self.mode = AppMode::Rendered;
    }

    /// Load `index`'s messages into the processor and reset interaction state.
    ///
    /// This is the shared core of entering a sample ([`Self::select_sample`]) and
    /// walking the sample list while keeping the split view open (↑/↓ in list
    /// focus). It does NOT touch `mode` or `panel_focus` — callers decide those.
    fn load_sample(&mut self, index: usize) {
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
        self.list_state.select(Some(index));

        // Process all messages at once.
        self.process_remaining_messages();
        self.rebuild_focus();
    }

    /// Process all remaining unprocessed messages.
    fn process_remaining_messages(&mut self) {
        while self.messages_processed < self.current_messages.len() {
            let msg = self.current_messages[self.messages_processed].clone();
            let _ = self.processor.process_message(msg);
            self.messages_processed += 1;
        }
    }

    /// Reset and replay all messages for the current sample.
    fn replay_current_sample(&mut self) {
        let messages = self.current_messages.clone();
        self.processor.reset();
        self.current_messages = messages;
        self.messages_processed = 0;
        self.focus_manager.reset();

        self.process_remaining_messages();
        self.rebuild_focus();
    }

    /// Rebuild the focus list from the first available surface.
    fn rebuild_focus(&mut self) {
        if let Some(surface) = self.processor.model.surfaces().next() {
            let components = surface.components.borrow();
            self.focus_manager.rebuild_from_components(&components);
        }
    }
}

// ---------------------------------------------------------------------------
// Free rendering functions (operate on extracted data, not on GalleryApp)
// ---------------------------------------------------------------------------

/// Render the sample list (full screen).
fn render_sample_list(
    frame: &mut ratatui::Frame,
    fd: &FrameData,
    list_state: &mut ListState,
) {
    let area = frame.area();
    let items: Vec<ListItem> = fd
        .samples
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            let text_style = if i == fd.selected_sample {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            // Index is always dim so the row number stays scannable regardless
            // of selection; the name/description carry the selection styling.
            let line = Line::from(vec![
                Span::styled(format!(" {:>2}. ", i + 1), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} — {}", name, desc), text_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" A2UI Gallery — Sample Browser "),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, list_state);
}

/// Render the split view: sample list on the left, surface on the right.
fn render_split_view(
    frame: &mut ratatui::Frame,
    fd: &FrameData,
    list_state: &mut ListState,
    surface: Option<&a2ui_core::model::surface_model::SurfaceModel>,
    registry: &ComponentRegistry,
    catalog: &Catalog,
    focused_id: Option<&str>,
) {
    let area = frame.area();

    // Split into: main panels (95%) and bottom bar (min 1 row)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(95), Constraint::Min(1)])
        .split(area);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);

    // Left: compact sample list.
    render_sample_list_panel(frame, fd, panels[0], list_state, fd.panel_focus == PanelFocus::List);

    // Right: rendered surface.
    render_surface_panel(frame, panels[1], surface, registry, catalog, focused_id, fd.panel_focus == PanelFocus::Render);

    // Bottom: controls help.
    render_help_bar(frame, outer[1], fd);
}

/// Render the sample list in a side panel (compact).
fn render_sample_list_panel(
    frame: &mut ratatui::Frame,
    fd: &FrameData,
    area: Rect,
    list_state: &mut ListState,
    focused: bool,
) {
    let items: Vec<ListItem> = fd
        .samples
        .iter()
        .enumerate()
        .map(|(i, (name, _desc))| {
            let text_style = if i == fd.selected_sample {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::styled(format!("{:>2}. ", i + 1), Style::default().fg(Color::DarkGray)),
                Span::styled(name.clone(), text_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let title = if focused { " ◄ Samples " } else { " Samples " };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, list_state);
}

/// Render the current surface using SurfaceRenderer.
///
/// A bordered frame is always drawn so the panel's focus state is visible
/// (yellow border + ` Surface ► ` title when focused); the surface itself is
/// rendered into the inner area so it never overwrites the border.
fn render_surface_panel(
    frame: &mut ratatui::Frame,
    area: Rect,
    surface: Option<&a2ui_core::model::surface_model::SurfaceModel>,
    registry: &ComponentRegistry,
    catalog: &Catalog,
    focused_id: Option<&str>,
    focused: bool,
) {
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let title = if focused { " Surface ► " } else { " Surface " };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(surface) = surface {
        let renderer = SurfaceRenderer::new(surface, registry, catalog);
        renderer.render(frame, inner, focused_id);
    } else {
        let paragraph = Paragraph::new("No surface loaded.\nPress 'n' to step through messages.");
        frame.render_widget(paragraph, inner);
    }
}

/// Render the bottom help bar.
fn render_help_bar(frame: &mut ratatui::Frame, area: Rect, fd: &FrameData) {
    let step_info = |prefix: &str| -> String {
        if fd.total_messages == 0 {
            String::new()
        } else {
            format!("{}[{}/{}] ", prefix, fd.messages_processed, fd.total_messages)
        }
    };

    let help_text: String = match fd.mode {
        AppMode::SampleList => {
            " ↑/k: up  ↓/j: down  Enter: select  q/Esc: quit ".to_string()
        }
        AppMode::Rendered => match fd.panel_focus {
            PanelFocus::List => format!(
                " [List ◄] ↑/↓: switch sample  Tab/Enter: focus surface  Esc: browser  q: quit {}",
                step_info("")
            ),
            PanelFocus::Render => format!(
                " [Surface ►] n: step  a: all  r: replay  Tab: cycle focus  Esc: back to list  q: quit {}",
                step_info("")
            ),
        },
    };

    let paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
