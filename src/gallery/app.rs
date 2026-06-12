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

use crate::core::catalog::Catalog;
use crate::core::message_processor::MessageProcessor;
use crate::core::protocol::server_to_client::A2uiMessage;
use crate::gallery::sample_loader::{self, Sample};
use crate::tui::catalogs::minimal::{build_minimal_catalog, build_minimal_registry};
use crate::tui::component_impl::ComponentRegistry;
use crate::tui::focus_manager::FocusManager;
use crate::tui::surface::SurfaceRenderer;

/// Path to the sample JSON files.
const SAMPLE_DIR: &str =
    "/home/liangdi/workspace/ai/a2ui/specification/v1_0/catalogs/minimal/examples";

/// Application mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppMode {
    /// Browsing the sample list (full screen).
    SampleList,
    /// Viewing a rendered sample (split panel).
    Rendered,
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
    /// List state for ratatui List widget highlighting.
    list_state: ListState,
}

impl GalleryApp {
    /// Create and initialize the gallery application.
    pub fn new() -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stderr());
        let terminal = Terminal::new(backend)?;

        let catalog = build_minimal_catalog();
        let registry = build_minimal_registry();
        let processor = MessageProcessor::new(vec![]);
        let samples = sample_loader::load_samples_from_dir(SAMPLE_DIR);

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
                        render_split_view(frame, &fd, list_state, surface_ref, registry, catalog);
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
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.mode = AppMode::SampleList;
            }
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
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Sample management
    // -----------------------------------------------------------------------

    /// Select a sample, switch to rendered mode, and process all messages.
    fn select_sample(&mut self, index: usize) {
        if index >= self.samples.len() {
            return;
        }

        // Reset processor state for the new sample.
        self.processor = MessageProcessor::new(vec![]);

        let sample = &self.samples[index];
        self.current_messages = sample.messages.clone();
        self.messages_processed = 0;
        self.focus_manager.reset();
        self.selected_sample = index;
        self.list_state.select(Some(index));

        // Process all messages at once.
        self.process_remaining_messages();
        self.rebuild_focus();

        self.mode = AppMode::Rendered;
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
        self.processor = MessageProcessor::new(vec![]);
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
            let style = if i == fd.selected_sample {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let line = Line::from(Span::styled(
                format!("  {} — {}", name, desc),
                style,
            ));
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
    surface: Option<&crate::core::model::surface_model::SurfaceModel>,
    registry: &ComponentRegistry,
    catalog: &Catalog,
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
    render_sample_list_panel(frame, fd, panels[0], list_state);

    // Right: rendered surface.
    render_surface_panel(frame, panels[1], surface, registry, catalog);

    // Bottom: controls help.
    render_help_bar(frame, outer[1], fd);
}

/// Render the sample list in a side panel (compact).
fn render_sample_list_panel(
    frame: &mut ratatui::Frame,
    fd: &FrameData,
    area: Rect,
    list_state: &mut ListState,
) {
    let items: Vec<ListItem> = fd
        .samples
        .iter()
        .enumerate()
        .map(|(i, (name, _desc))| {
            let style = if i == fd.selected_sample {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let line = Line::from(Span::styled(format!(" {} ", name), style));
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Samples "),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, list_state);
}

/// Render the current surface using SurfaceRenderer.
fn render_surface_panel(
    frame: &mut ratatui::Frame,
    area: Rect,
    surface: Option<&crate::core::model::surface_model::SurfaceModel>,
    registry: &ComponentRegistry,
    catalog: &Catalog,
) {
    if let Some(surface) = surface {
        let renderer = SurfaceRenderer::new(surface, registry, catalog);
        renderer.render(frame, area);
    } else {
        let paragraph = Paragraph::new("No surface loaded.\nPress 'n' to step through messages.")
            .block(Block::default().borders(Borders::ALL).title(" Surface "));
        frame.render_widget(paragraph, area);
    }
}

/// Render the bottom help bar.
fn render_help_bar(frame: &mut ratatui::Frame, area: Rect, fd: &FrameData) {
    let help_text: String = match fd.mode {
        AppMode::SampleList => {
            " ↑/k: up  ↓/j: down  Enter: select  q/Esc: quit ".to_string()
        }
        AppMode::Rendered => {
            let step_info = if fd.total_messages == 0 {
                String::new()
            } else {
                format!(" [{}/{}]", fd.messages_processed, fd.total_messages)
            };
            format!(
                " n: step  a: all  r: replay  Tab: focus  Esc: back{} ",
                step_info
            )
        }
    };

    let paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
