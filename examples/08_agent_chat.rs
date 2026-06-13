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
//! - Available commands: hello, weather, tasks, story, help
//! - Mouse wheel or Arrow keys to scroll chat history
//! - PageUp / PageDown to scroll by page
//! - `q` or `Ctrl+C` to quit

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
// Mock agent: generate A2UI protocol messages
// ---------------------------------------------------------------------------

const CATALOG_ID: &str =
    "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";

fn generate_response(sid: &str, user_msg: &str) -> Vec<serde_json::Value> {
    let lower = user_msg.to_lowercase();
    if lower.contains("hello") || lower.contains("hi") || lower == "hey" {
        scenario_greeting(sid)
    } else if lower.contains("weather") {
        scenario_weather(sid)
    } else if lower.contains("task") {
        scenario_tasks(sid)
    } else if lower.contains("story") || lower.contains("tell me") {
        scenario_streaming(sid)
    } else if lower.contains("help") || lower.contains("command") {
        scenario_help(sid)
    } else {
        scenario_default(sid)
    }
}

fn scenario_greeting(sid: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{"text":""}}}),
        serde_json::json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["greeting","sub","divider","hint"],"align":"stretch"},
            {"id":"greeting","component":"Text","text":"Hello there! 👋","variant":"h2"},
            {"id":"sub","component":"Text","text":"I'm your A2UI Agent. I can show you rich UI components streamed via the A2UI protocol!","variant":"body"},
            {"id":"divider","component":"Divider","axis":"horizontal"},
            {"id":"hint","component":"Text","text":"Try: weather, tasks, story, or help","variant":"caption"}
        ]}}),
    ]
}

fn scenario_weather(sid: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        serde_json::json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["intro","card"],"align":"stretch"},
            {"id":"intro","component":"Text","text":"Here's the weather forecast:","variant":"body"},
            {"id":"card","component":"Card","child":"card_inner","weight":8},
            {"id":"card_inner","component":"Column","children":["city","temp","cond","hum","wind","d1","foot"],"align":"stretch"},
            {"id":"city","component":"Text","text":"📍 San Francisco, CA","variant":"h3"},
            {"id":"temp","component":"Text","text":"🌡️  Temperature: 72°F (22°C)","variant":"body"},
            {"id":"cond","component":"Text","text":"🌤️  Condition: Partly Cloudy","variant":"body"},
            {"id":"hum","component":"Text","text":"💧 Humidity: 65%","variant":"body"},
            {"id":"wind","component":"Text","text":"💨 Wind: 12 mph NW","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"foot","component":"Text","text":"📅 7-Day forecast available | 🔄 Updated 2:30 PM","variant":"caption"}
        ]}}),
    ]
}

fn scenario_tasks(sid: &str) -> Vec<serde_json::Value> {
    let mut messages = vec![
        serde_json::json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{
            "progress_bar": "░░░░░░░░░░░░░░░░░░░░ 0%",
            "status": "⏳ Scanning project...",
            "task_text": "",
            "summary_text": "loading..."
        }}}),
        serde_json::json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["title","progress_card","d1","task_card","d2","footer"],"align":"stretch"},
            {"id":"title","component":"Text","text":"🚀 Sprint Board — a2ui v0.2.0","variant":"h1"},
            {"id":"progress_card","component":"Card","child":"progress_inner","weight":3},
            {"id":"progress_inner","component":"Column","children":["bar","status"],"align":"stretch"},
            {"id":"bar","component":"Text","text":{"path":"/progress_bar"},"variant":"h3"},
            {"id":"status","component":"Text","text":{"path":"/status"},"variant":"caption"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"task_card","component":"Card","child":"task_inner","weight":10},
            {"id":"task_inner","component":"Column","children":["task_header","task_text"],"align":"stretch"},
            {"id":"task_header","component":"Text","text":"📝 Tasks","variant":"h3"},
            {"id":"task_text","component":"Text","text":{"path":"/task_text"},"variant":"body"},
            {"id":"d2","component":"Divider","axis":"horizontal"},
            {"id":"footer","component":"Text","text":{"path":"/summary_text"},"variant":"caption"}
        ]}}),
    ];

    let tasks = vec![
        ("🔴 P0", "✅", "Fix layout engine justify bug"),
        ("🔴 P0", "✅", "Implement focus management"),
        ("🟡 P1", "✅", "Add Card component shadow"),
        ("🟡 P1", "⬜", "SSE transport layer"),
        ("🟢 P2", "⬜", "WebSocket bidirectional support"),
        ("🟢 P2", "⬜", "Agent chat streaming demo"),
        ("🔵 P3", "⬜", "Integration test suite"),
        ("🔵 P3", "⬜", "CSS theme engine"),
    ];

    let total = tasks.len();
    let mut completed = 0usize;

    messages.push(serde_json::json!({"version":"v1.0","updateDataModel":{"surfaceId":sid,"path":"/status","value":"⏳ Scanning 24 files..."}}));

    for (i, (_priority, status, _name)) in tasks.iter().enumerate() {
        if *status == "✅" { completed += 1; }

        let pct = (i + 1) * 100 / total;
        let filled = pct / 5;
        let empty = 20 - filled;
        let bar: String = "█".repeat(filled) + &"░".repeat(empty);

        let lines: Vec<String> = tasks[..=i]
            .iter()
            .map(|(pri, st, n)| {
                let check = if *st == "✅" { "✅" } else { "⬜" };
                format!("  {} {} {}", check, pri, n)
            })
            .collect();

        let stat = if i < total - 1 {
            format!("⏳ Processing task {}/{}", i + 1, total)
        } else {
            "✅ All tasks loaded!".to_string()
        };

        let summary = format!(
            "{} done · {} remaining · {}% complete",
            completed, total - completed, completed * 100 / total
        );

        messages.push(serde_json::json!({
            "version":"v1.0",
            "updateDataModel":{"surfaceId":sid,"path":"/","value":{
                "progress_bar": format!("{} {}%", bar, pct),
                "status": stat,
                "task_text": lines.join("\n"),
                "summary_text": summary
            }}
        }));
    }

    messages
}

fn scenario_streaming(sid: &str) -> Vec<serde_json::Value> {
    let story = "Once upon a time, in a digital realm far away, there lived a protocol \
        called A2UI. It could transform plain JSON messages into beautiful user interfaces, \
        streaming them in real-time across the wire. Developers marveled at its simplicity \
        — no build steps, no bundlers, just pure structured data flowing from agent to screen. 🌟";

    let words: Vec<&str> = story.split(' ').collect();
    let mut messages = vec![
        serde_json::json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{"story":""}}}),
        serde_json::json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["label","story_text"],"align":"stretch"},
            {"id":"label","component":"Text","text":"📖 A Story (streaming word-by-word via updateDataModel)","variant":"h3"},
            {"id":"story_text","component":"Text","text":{"path":"/story"},"variant":"body"}
        ]}}),
    ];

    let mut accumulated = String::new();
    for word in words {
        if !accumulated.is_empty() {
            accumulated.push(' ');
        }
        accumulated.push_str(word);
        messages.push(serde_json::json!({
            "version":"v1.0",
            "updateDataModel":{"surfaceId":sid,"path":"/story","value": accumulated}
        }));
    }
    messages
}

fn scenario_help(sid: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        serde_json::json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["title","d0","c1","c2","c3","c4","c5","d1","hint"],"align":"stretch"},
            {"id":"title","component":"Text","text":"📖 Available Commands","variant":"h2"},
            {"id":"d0","component":"Divider","axis":"horizontal"},
            {"id":"c1","component":"Text","text":"  hello   → Simple greeting response","variant":"body"},
            {"id":"c2","component":"Text","text":"  weather → Weather card with rich components","variant":"body"},
            {"id":"c3","component":"Text","text":"  tasks   → Interactive task list in a Card","variant":"body"},
            {"id":"c4","component":"Text","text":"  story   → Streaming text word-by-word","variant":"body"},
            {"id":"c5","component":"Text","text":"  help    → Show this command list","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"hint","component":"Text","text":"Each response is streamed as A2UI protocol messages (text/a2ui over SSE)","variant":"caption"}
        ]}}),
    ]
}

fn scenario_default(sid: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        serde_json::json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["msg","d1","card"],"align":"stretch"},
            {"id":"msg","component":"Text","text":"I received your message! Here are some things you can try:","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"card","component":"Card","child":"card_inner","weight":5},
            {"id":"card_inner","component":"Column","children":["s1","s2","s3","s4"],"align":"stretch"},
            {"id":"s1","component":"Text","text":"💬  Say \"hello\" for a greeting","variant":"body"},
            {"id":"s2","component":"Text","text":"🌤️  Say \"weather\" for a weather card","variant":"body"},
            {"id":"s3","component":"Text","text":"📋  Say \"tasks\" for a task list","variant":"body"},
            {"id":"s4","component":"Text","text":"📖  Say \"story\" for streaming text","variant":"body"}
        ]}}),
    ]
}

// ---------------------------------------------------------------------------
// Estimate how many terminal rows a surface needs
// ---------------------------------------------------------------------------

fn estimate_surface_height(processor: &MessageProcessor, surface_id: &str) -> u16 {
    let surface = match processor.model.get_surface(surface_id) {
        Some(s) => s,
        None => return 2,
    };
    if !surface.has_root() {
        return 2;
    }
    let components = surface.components.borrow();
    let count = components.len() as u16;

    // Count visual rows: leaf components (Text, Divider) = 1 row each.
    // Cards add 2 border rows + 2 margin rows = 4 overhead.
    // Container components (Column, Row) = 0 rows (layout only).
    // We can't iterate individual component types, so use a heuristic:
    // ~2 rows per component is reasonable (leaves=1, cards=4, containers=0).
    // Weighted cards with many children need more, so add buffer.
    (count * 2).max(4)
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
        " Enter: send | ↑↓/Mouse: scroll | q: quit".to_string()
    };
    let help_style = if streaming {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(Paragraph::new(help_text).style(help_style), chunks[0]);

    let prompt = if input.is_empty() && !streaming {
        " > Type a message (hello, weather, tasks, story, help)...".to_string()
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
    let welcome_create = serde_json::json!({
        "version":"v1.0",
        "createSurface":{"surfaceId":&welcome_sid,"catalogId":CATALOG_ID,"dataModel":{}}
    });
    let welcome_update = serde_json::json!({
        "version":"v1.0",
        "updateComponents":{"surfaceId":&welcome_sid,"components":[
            {"id":"root","component":"Column","children":["title","sub","d1","hint"],"align":"stretch"},
            {"id":"title","component":"Text","text":"🤖 Welcome to A2UI Agent Chat!","variant":"h1"},
            {"id":"sub","component":"Text","text":"This is a terminal AI chat interface powered by the A2UI protocol.","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"hint","component":"Text","text":"Type a message below to get started. Try: hello, weather, tasks, story","variant":"caption"}
        ]}
    });
    processor.process_message(MessageProcessor::parse_message(&welcome_create.to_string())?)?;
    processor.process_message(MessageProcessor::parse_message(&welcome_update.to_string())?)?;
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
                    "ai" => estimate_surface_height(&processor, &e.surface_id) as usize,
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
                                renderer.render(frame, rect, None);
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
                        KeyCode::Char('q') if input.is_empty() && !streaming => {
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
