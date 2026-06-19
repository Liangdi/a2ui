//! # Example: A2UI Agent Chat (TUI)
//!
//! Demonstrates an AI agent chat interface rendered entirely in the terminal.
//! A mock agent generates A2UI protocol messages (simulating SSE/text-a2ui
//! streaming), and the TUI progressively renders them as chat messages.
//!
//! ## What it demonstrates
//! - Multiple surfaces (one per AI message) rendered in a chat layout
//! - Progressive A2UI message streaming: createSurface → updateComponents → updateDataModel
//! - Rich component rendering: Text, Card, Column, Row, Divider
//! - Streaming text (word-by-word via updateDataModel)
//! - Interactive text input for sending messages
//!
//! ## Run
//! ```sh
//! cargo run --example 08_agent_chat
//! ```
//!
//! ## Controls
//! - Type a message and press Enter to send
//! - Available commands: hello, weather, tasks, story, stats, quote, help
//! - Mouse wheel or Arrow keys to scroll chat history
//! - PageUp / PageDown to scroll by page
//! - `Esc` or `Ctrl+C` to quit

use std::io;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};

use a2ui::core::catalog::Catalog;
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::agent_chat::{generate_response, welcome_messages};
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

// ---------------------------------------------------------------------------
// Chat entry — one per message in the conversation
// ---------------------------------------------------------------------------

struct ChatEntry {
    role: String,       // "user" or "ai"
    surface_id: String, // empty for user messages
    text: String,       // user message text
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn render_user_entry(frame: &mut Frame, area: Rect, text: &str) {
    let content = Line::from(vec![
        Span::styled(
            " 👤 You ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(text, Style::default().fg(Color::White)),
    ]);
    frame.render_widget(Paragraph::new(content), area);
}

fn render_input(frame: &mut Frame, area: Rect, input: &str, streaming: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(area);

    let help_text = if streaming {
        " ⏳ Streaming A2UI messages...".to_string()
    } else {
        " Enter: send | ↑↓/Mouse: scroll | Esc: quit".to_string()
    };
    let help_style = if streaming {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(Paragraph::new(help_text).style(help_style), chunks[0]);

    let prompt = if input.is_empty() && !streaming {
        " > Type a message (hello, weather, tasks, story, stats, quote, help)...".to_string()
    } else {
        format!(" > {}█", input)
    };
    let input_style = if streaming {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let input_block = Block::default()
        .borders(Borders::TOP)
        .style(Style::default().fg(Color::DarkGray));
    let inner = input_block.inner(chunks[1]);
    frame.render_widget(input_block, chunks[1]);
    frame.render_widget(Paragraph::new(prompt).style(input_style), inner);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = build_basic_registry();
    let render_catalog = Catalog::new("placeholder");
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    // Chat state
    let mut entries: Vec<ChatEntry> = Vec::new();
    let mut input = String::new();
    let mut msg_counter: u32 = 0;
    let mut pending_messages: Vec<serde_json::Value> = Vec::new();
    let mut pending_timer: u8 = 0;
    let mut typing = false;

    // Scroll state: stores offset from the bottom (0 = scrolled to bottom)
    let mut scroll_from_bottom: usize = 0;
    // Flag: auto-scroll to bottom when new content arrives
    let mut auto_scroll = true;

    // ── Welcome message ────────────────────────────────────────────────
    let welcome_sid = "welcome".to_string();
    for msg in welcome_messages(&welcome_sid) {
        let json = serde_json::to_string(&msg).unwrap_or_default();
        processor.process_message(MessageProcessor::parse_message(&json)?)?;
    }
    entries.push(ChatEntry {
        role: "ai".into(),
        surface_id: welcome_sid,
        text: String::new(),
    });

    // ── Terminal setup ─────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // ── Main loop ──────────────────────────────────────────────────────
    loop {
        // Simulate SSE streaming: process one pending message per tick
        let streaming = !pending_messages.is_empty();
        let prev_entry_count = entries.len();

        if pending_timer > 0 {
            pending_timer -= 1;
        } else if let Some(msg) = pending_messages.first().cloned() {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if let Ok(parsed) = MessageProcessor::parse_message(&json) {
                let sid = msg
                    .get("createSurface")
                    .and_then(|cs| cs.get("surfaceId"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if let Some(ref sid) = sid {
                    entries.push(ChatEntry {
                        role: "ai".into(),
                        surface_id: sid.clone(),
                        text: String::new(),
                    });
                    typing = false;
                }

                let _ = processor.process_message(parsed);
            }
            pending_messages.remove(0);
            pending_timer = 1;
        } else if typing {
            typing = false;
        }

        // Auto-scroll: reset scroll when new entries arrive
        if entries.len() > prev_entry_count || auto_scroll {
            scroll_from_bottom = 0;
            auto_scroll = true;
        }

        // ── Render ─────────────────────────────────────────────────────
        terminal.draw(|frame| {
            let area = frame.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(8), Constraint::Length(3)])
                .split(area);

            let chat_area = chunks[0];
            let vh = chat_area.height as usize;

            // ── Calculate each entry's desired height ──────────────────
            let mut entry_heights: Vec<usize> = entries
                .iter()
                .map(|e| match e.role.as_str() {
                    "user" => 1,
                    "ai" => {
                        let w = chat_area.width.saturating_sub(1);
                        match processor.model.get_surface(&e.surface_id) {
                            Some(surface) if surface.has_root() => {
                                a2ui::tui::surface::SurfaceRenderer::new(
                                    surface,
                                    &registry,
                                    &render_catalog,
                                )
                                .measure(w)
                                .unwrap_or(4) as usize
                            }
                            _ => 2,
                        }
                    }
                    _ => 0,
                })
                .collect();

            if typing {
                entry_heights.push(1);
            }

            let total_h: usize = entry_heights.iter().sum();

            // ── Calculate scroll offset ────────────────────────────────
            let max_scroll = total_h.saturating_sub(vh);
            scroll_from_bottom = scroll_from_bottom.min(max_scroll);
            let scroll_offset = max_scroll.saturating_sub(scroll_from_bottom);

            // ── Render entries with absolute y-positioning ──────────────
            let mut y: usize = 0;
            let entries_len = entries.len();

            for (idx, &h) in entry_heights.iter().enumerate() {
                let entry_top = y;
                let entry_bot = y + h;
                y += h;

                if entry_bot <= scroll_offset {
                    continue;
                }
                if entry_top >= scroll_offset + vh {
                    break;
                }

                let vis_top = entry_top.saturating_sub(scroll_offset);
                let vis_bot = std::cmp::min(entry_bot.saturating_sub(scroll_offset), vh);
                let vis_h = vis_bot - vis_top;

                let rect = Rect {
                    x: chat_area.x,
                    y: chat_area.y + vis_top as u16,
                    width: chat_area.width.saturating_sub(1),
                    height: vis_h as u16,
                };

                if rect.width == 0 || rect.height == 0 {
                    continue;
                }

                if idx >= entries_len {
                    if typing {
                        let dots = Paragraph::new(Line::from(vec![
                            Span::styled(
                                " 🤖 AI is thinking",
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::raw(" ..."),
                        ]));
                        frame.render_widget(dots, rect);
                    }
                    continue;
                }

                let entry = &entries[idx];
                match entry.role.as_str() {
                    "user" => {
                        render_user_entry(frame, rect, &entry.text);
                    }
                    "ai" => {
                        if let Some(surface) =
                            processor.model.get_surface(&entry.surface_id)
                        {
                            if surface.has_root() {
                                let renderer =
                                    a2ui::tui::surface::SurfaceRenderer::new(
                                        surface,
                                        &registry,
                                        &render_catalog,
                                    );
                                // Rows of *this* surface scrolled above the
                                // viewport. The library renders the surface at its
                                // natural height off-screen and blits the visible
                                // slice, so scrolling reveals a true (un-squished)
                                // slice instead of reflowing the content.
                                let src_skip = scroll_offset.saturating_sub(entry_top);
                                renderer.render_scrolled(frame, rect, src_skip, None);
                            }
                        }
                    }
                    _ => {}
                }
            }

            // ── Scrollbar (ratatui built-in) ──────────────────────────
            if total_h > vh {
                let mut scrollbar_state = ScrollbarState::new(total_h)
                    .position(scroll_offset)
                    .viewport_content_length(vh);

                frame.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight)
                        .style(Style::default().fg(Color::DarkGray)),
                    chat_area,
                    &mut scrollbar_state,
                );
            }

            // ── Input area ─────────────────────────────────────────────
            render_input(frame, chunks[1], &input, streaming);
        })?;

        // ── Handle events (keyboard + mouse) ──────────────────────────
        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('c')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            break;
                        }
                        // `Ctrl+Q` also quits (a bare `q` must be free to type,
                        // e.g. the "quote" command — otherwise the first `q`
                        // would exit the app whenever the input is empty).
                        KeyCode::Char('q')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            break;
                        }
                        KeyCode::Esc if !streaming => {
                            break;
                        }
                        KeyCode::Enter if !streaming => {
                            let msg = input.trim().to_string();
                            if msg.is_empty() {
                                continue;
                            }
                            input.clear();

                            entries.push(ChatEntry {
                                role: "user".into(),
                                surface_id: String::new(),
                                text: msg.clone(),
                            });

                            typing = true;
                            auto_scroll = true;
                            scroll_from_bottom = 0;
                            msg_counter += 1;
                            let sid = format!("msg-{}", msg_counter);
                            pending_messages = generate_response(&sid, &msg);
                            pending_timer = 2;
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Char(c) => {
                            if input.len() < 200 {
                                input.push(c);
                            }
                        }
                        // Scroll: Arrow keys
                        KeyCode::Up => {
                            scroll_from_bottom += 3;
                            auto_scroll = false;
                        }
                        KeyCode::Down => {
                            scroll_from_bottom = scroll_from_bottom.saturating_sub(3);
                            if scroll_from_bottom == 0 {
                                auto_scroll = true;
                            }
                        }
                        // Scroll: PageUp / PageDown
                        KeyCode::PageUp => {
                            scroll_from_bottom += 20;
                            auto_scroll = false;
                        }
                        KeyCode::PageDown => {
                            scroll_from_bottom = scroll_from_bottom.saturating_sub(20);
                            if scroll_from_bottom == 0 {
                                auto_scroll = true;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            scroll_from_bottom += 3;
                            auto_scroll = false;
                        }
                        MouseEventKind::ScrollDown => {
                            scroll_from_bottom = scroll_from_bottom.saturating_sub(3);
                            if scroll_from_bottom == 0 {
                                auto_scroll = true;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    // ── Cleanup ────────────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
