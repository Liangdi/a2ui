//! # Example: A2UI Agent Chat — Iced backend
//!
//! An AI-agent chat window rebuilt on the a2ui protocol, rendered into a real
//! OS window by the Iced backend. This is the Iced counterpart of the ratatui
//! [`08_agent_chat`]: same mock agent, same scenarios, same per-message surface
//! model — different renderer.
//!
//! A mock agent streams A2UI protocol messages (simulating `text/a2ui` SSE).
//! Each AI response is a **separate** a2ui surface: a `createSurface` message
//! opens it, then `updateComponents` / `updateDataModel` messages populate it.
//! Every AI chat entry is rendered through the Iced generic walker
//! ([`render_node`]) so the whole conversation is a vertical column of
//! independent surfaces — exactly as in the terminal version, but drawn with
//! real GUI widgets. A `text_input` at the bottom sends user messages.
//!
//! ## What it demonstrates
//! - Multiple surfaces (one per AI message) rendered side-by-side in a chat
//!   layout, each through the same generic walker the gallery uses.
//! - Progressive A2UI streaming driven by an Iced [`Subscription`]: a
//!   background thread emits a [`Message::Tick`] every ~100 ms, and `update`
//!   feeds one pending protocol message per tick (`createSurface` opens a new
//!   chat entry; the rest populate it).
//! - The Elm split: `view` is an immutable read of the processor + entry list,
//!   `update` is the only writer — no state bridge, no diffing (the Iced
//!   backend's defining strength, versus egui's `EditBuffers` bridge).
//! - Chat-bubble styling, a disabled-while-streaming input, and auto-scroll to
//!   the newest entry via [`scrollable::scroll_to`].
//!
//! [`08_agent_chat`]: ../../a2ui/examples/08_agent_chat.rs
//! [`render_node`]: a2ui_iced::walker::render_node
//! [`Subscription`]: iced::Subscription
//! [`Message::Tick`]: Message::Tick
//! [`scrollable::scroll_to`]: iced::widget::scrollable::scroll_to
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-iced --example 08_agent_chat --features backend
//! ```
//!
//! ## Controls
//! - Type a message and press Enter to send
//! - Available commands: hello, weather, tasks, story, stats, quote, help
//! - Close the window (or the OS window-close button) to quit

use std::collections::HashMap;
use std::time::Duration;

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_tui::agent_chat::{generate_response, welcome_messages};
use a2ui_tui::catalogs::basic::build_basic_catalog;

use a2ui_iced::walker::render_node;

use iced::widget::image;
use iced::widget::operation::AbsoluteOffset;
use iced::widget::{Column, container, row, scrollable, text, text_input};
use iced::{
    Alignment, Background, Border, Color, Element, Fill, Font, Length, Padding, Subscription, Task,
    Theme, widget::Id,
};

// ─── Palette ─────────────────────────────────────────────────────────────────

/// User-bubble accent.
const USER_ACCENT: Color = Color::from_rgb(0.337, 0.941, 1.0);
/// AI-bubble accent.
const AI_ACCENT: Color = Color::from_rgb(0.365, 1.0, 0.690);
/// Thinking / status text.
const DIM: Color = Color::from_rgb(0.5, 0.55, 0.6);
/// Body text.
const TEXT: Color = Color::from_rgb(0.92, 0.94, 0.97);
/// Chat-pane background.
const BG: Color = Color::from_rgb(0.10, 0.11, 0.14);
/// Bubble background.
const BUBBLE: Color = Color::from_rgb(0.16, 0.18, 0.22);

// ─── Chat model ──────────────────────────────────────────────────────────────

/// One row of the conversation. AI rows render their own surface; user rows
/// carry only text.
struct ChatEntry {
    /// `"user"` or `"ai"`.
    role: String,
    /// Surface id for AI rows (empty for user rows).
    surface_id: String,
    /// User-typed text (empty for AI rows).
    text: String,
}

// ─── The Elm application ─────────────────────────────────────────────────────

/// The chat's runtime state. Mirrors the ratatui version's locals, plus the
/// Iced-specific image cache / local-tab maps the walker needs.
struct ChatApp {
    processor: MessageProcessor,
    functions: HashMap<String, Box<dyn FunctionImplementation>>,
    entries: Vec<ChatEntry>,
    input: String,
    msg_counter: u32,
    /// The simulated SSE queue: protocol messages not yet fed to the processor.
    pending_messages: Vec<serde_json::Value>,
    /// Ticks to wait before feeding the next pending message (pacing).
    pending_timer: u8,
    /// True while we are waiting for the first `createSurface` of a response.
    typing: bool,
    /// Remote-image cache (empty here — the chat surfaces are static — but the
    /// walker requires it).
    image_cache: HashMap<String, Option<image::Handle>>,
    /// Locally-tracked active tab per Tabs component (unused by the scenarios).
    local_tabs: HashMap<String, usize>,
    /// Number of entries rendered last frame; used to detect growth and fire
    /// auto-scroll only when new content actually arrives.
    last_seen_entries: usize,
    /// The chat scrollable's id, for `scroll_to` commands.
    scroll_id: Id,
}

/// The interactions this app produces: clock ticks from the background
/// subscription, the text input's value, an Enter-to-send, and (absorbed)
/// interaction messages from the walker-rendered chat surfaces.
#[derive(Debug, Clone)]
enum Message {
    /// ~100 ms clock tick from the background subscription — drives streaming.
    Tick,
    /// `text_input::on_input` payload.
    Input(String),
    /// Enter pressed in the input (or a send button).
    Send,
    /// A forwarded interaction from a walker-rendered surface. The chat
    /// scenarios are static (no buttons/tabs that mutate state), so this is
    /// collected and dropped in `update` — the variant exists only so the
    /// walker's `Element<a2ui_iced::Message>` can be `.map`ped into the app's
    /// own message type.
    Surface,
}

impl ChatApp {
    /// Boot: build the processor seeded with the basic catalog + function map,
    /// feed the welcome surface immediately, and push its AI chat entry.
    fn new(functions: HashMap<String, Box<dyn FunctionImplementation>>) -> Self {
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

        // Feed the welcome surface up front so it is fully rendered by the
        // first `view` (the ratatui version streams it too, but doing it
        // synchronously here keeps the welcome bubble from flashing in).
        let welcome_sid = "welcome".to_string();
        for msg in welcome_messages(&welcome_sid) {
            feed(&mut processor, &msg);
        }

        let entries = vec![ChatEntry {
            role: "ai".into(),
            surface_id: welcome_sid,
            text: String::new(),
        }];

        Self {
            processor,
            functions,
            entries,
            input: String::new(),
            msg_counter: 0,
            pending_messages: Vec::new(),
            pending_timer: 0,
            typing: false,
            image_cache: HashMap::new(),
            local_tabs: HashMap::new(),
            last_seen_entries: 0,
            scroll_id: Id::new("chat-scroll"),
        }
    }

    /// Apply one message: a Tick advances the simulated stream by one protocol
    /// message; Input/Send mutate the composer and kick off a new response.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.step_stream();
            }
            Message::Input(s) => {
                // Ignore edits while a response is streaming (matches the
                // ratatui version, which disables the input while streaming).
                if self.pending_messages.is_empty() && self.pending_timer == 0 {
                    self.input = s;
                }
            }
            Message::Send => self.send(),
            // Static chat surfaces: no widget interaction mutates app state.
            Message::Surface => {}
        }

        // Auto-scroll to the newest entry whenever the list grows. Emitting it
        // every tick is harmless (Iced dedups), but gating on growth keeps the
        // user's manual scroll position stable between streamed updates.
        if self.entries.len() > self.last_seen_entries {
            self.last_seen_entries = self.entries.len();
            // Anchor to the bottom of the content: a large absolute offset
            // pushes the viewport past the end, which Iced clamps to the last
            // row — the simplest portable "scroll to bottom".
            return iced::widget::operation::scroll_to(
                self.scroll_id.clone(),
                AbsoluteOffset {
                    x: 0.0,
                    y: f32::MAX,
                },
            );
        }

        Task::none()
    }

    /// Advance the simulated stream by one protocol message (one per tick).
    fn step_stream(&mut self) {
        if self.pending_timer > 0 {
            self.pending_timer -= 1;
            return;
        }
        let Some(msg) = self.pending_messages.first().cloned() else {
            // Queue drained: the "thinking" indicator can come down.
            self.typing = false;
            return;
        };

        // A `createSurface` opens a new AI chat entry before the rest of the
        // scenario populates it.
        let new_sid = msg
            .get("createSurface")
            .and_then(|c| c.get("surfaceId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(sid) = new_sid {
            self.entries.push(ChatEntry {
                role: "ai".into(),
                surface_id: sid,
                text: String::new(),
            });
            self.typing = false;
        }

        feed(&mut self.processor, &msg);
        self.pending_messages.remove(0);
        // Pace the stream: one message, then a one-tick pause.
        self.pending_timer = 1;
    }

    /// Send the current input: push a user entry, pick a scenario, and prime
    /// the pending queue.
    fn send(&mut self) {
        let msg = self.input.trim().to_string();
        if msg.is_empty() || !self.pending_messages.is_empty() || self.typing {
            return;
        }
        self.input.clear();

        self.entries.push(ChatEntry {
            role: "user".into(),
            surface_id: String::new(),
            text: msg.clone(),
        });

        self.typing = true;
        self.msg_counter += 1;
        let sid = format!("msg-{}", self.msg_counter);
        self.pending_messages = generate_response(&sid, &msg);
        // Brief pause before the first protocol message lands, so the
        // "thinking" indicator is visible.
        self.pending_timer = 2;
    }

    /// Build the chat: a scrollable column of bubbles over a fixed input row.
    fn view(&self) -> Element<'_, Message> {
        let streaming = !self.pending_messages.is_empty() || self.typing;

        // ── Chat history ───────────────────────────────────────────────────
        let mut list = Column::new().spacing(10.0).width(Fill);

        for entry in &self.entries {
            list = list.push(match entry.role.as_str() {
                "user" => self.user_bubble(&entry.text),
                _ => self.ai_bubble(entry),
            });
        }

        if self.typing {
            list = list.push(self.thinking_indicator());
        }

        let chat = scrollable(list)
            .id(self.scroll_id.clone())
            .width(Fill)
            .height(Fill)
            .spacing(10.0);

        // ── Input row ──────────────────────────────────────────────────────
        let placeholder = "Type a message (hello, weather, tasks, story, stats, quote, help)…";
        let input = text_input(placeholder, &self.input)
            .on_input(Message::Input)
            .on_submit(Message::Send)
            .padding(10.0)
            .size(14.0);

        let status = if streaming {
            text("🤖 Streaming A2UI messages…")
                .color(DIM)
                .size(11.0)
                .font(Font::MONOSPACE)
        } else {
            text("Enter: send   ·   close window: quit")
                .color(DIM)
                .size(11.0)
                .font(Font::MONOSPACE)
        };

        let composer = Column::new()
            .spacing(4.0)
            .width(Fill)
            .push(input)
            .push(status);

        // ── Stack chat + composer ──────────────────────────────────────────
        let body = Column::new()
            .push(
                container(chat).width(Fill).height(Fill).padding(
                    Padding::new(0.0)
                        .top(12.0)
                        .bottom(0.0)
                        .left(16.0)
                        .right(16.0),
                ),
            )
            .push(
                container(composer).width(Fill).padding(
                    Padding::new(0.0)
                        .top(8.0)
                        .bottom(12.0)
                        .left(16.0)
                        .right(16.0),
                ),
            )
            .width(Fill)
            .height(Fill);

        container(body)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(BG)),
                ..container::Style::default()
            })
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Render an AI entry: its surface's `root` component through the generic
    /// walker, wrapped in a bubble. Falls back to a muted placeholder if the
    /// surface or its root is not ready yet.
    fn ai_bubble(&self, entry: &ChatEntry) -> Element<'_, Message> {
        let inner: Element<'_, Message> = match self.processor.model.get_surface(&entry.surface_id)
        {
            Some(surface) => {
                let dm = surface.data_model.borrow();
                let comps = surface.components.borrow();
                if comps.contains("root") {
                    // The walker returns `Element<a2ui_iced::Message>`; map
                    // each surface interaction into this app's `Message`
                    // (the chat scenarios are static, so they are dropped).
                    render_node(
                        "root",
                        &surface.id,
                        "",
                        &dm,
                        &comps,
                        &self.functions,
                        None,
                        &self.image_cache,
                        &self.local_tabs,
                    )
                    .map(|_| Message::Surface)
                } else {
                    text("…").color(DIM).into()
                }
            }
            None => text("(surface not ready)").color(DIM).into(),
        };

        bubble(inner, AI_ACCENT, Alignment::Start)
    }

    /// A user message: a one-line label, right-aligned, in the user accent.
    fn user_bubble(&self, text_content: &str) -> Element<'_, Message> {
        let label: Element<'_, Message> = text(format!("👤 You:  {text_content}"))
            .color(TEXT)
            .size(14.0)
            .into();
        bubble(label, USER_ACCENT, Alignment::End)
    }

    /// The "AI is thinking" placeholder.
    fn thinking_indicator(&self) -> Element<'_, Message> {
        let label: Element<'_, Message> = text("🤖 AI is thinking …")
            .color(DIM)
            .size(13.0)
            .font(Font::MONOSPACE)
            .into();
        bubble(label, AI_ACCENT, Alignment::Start)
    }
}

// ─── Bubble chrome ───────────────────────────────────────────────────────────

/// Wrap content in a rounded bubble and align it to one side of the column.
/// `align_x == End` (user bubbles) pushes the bubble right; `Start` (AI bubbles)
/// keeps it left. A thin accent stripe distinguishes the two.
fn bubble<'a>(
    content: impl Into<Element<'a, Message>>,
    accent: Color,
    align_x: Alignment,
) -> Element<'a, Message> {
    let body = container(content.into())
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(BUBBLE)),
            border: Border {
                color: accent,
                width: 2.0,
                radius: 10.0.into(),
            },
            ..container::Style::default()
        })
        .padding(Padding::from(10.0))
        .max_width(720.0)
        .width(Length::Shrink);

    // Two fill spacers flank the bubble; the alignment picks which one collapses
    // (width 0) so the bubble sits flush with the chosen edge.
    let left = iced::widget::Space::new().width(if align_x == Alignment::End {
        Fill
    } else {
        Length::Shrink
    });
    let right = iced::widget::Space::new().width(if align_x == Alignment::End {
        Length::Shrink
    } else {
        Fill
    });

    row![left, body, right]
        .align_y(Alignment::Center)
        .width(Fill)
        .into()
}

// ─── Feeding the processor ───────────────────────────────────────────────────

/// Serialize → parse → process one protocol message (the universal pattern).
fn feed(processor: &mut MessageProcessor, value: &serde_json::Value) {
    let json = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(_) => return,
    };
    let parsed = match MessageProcessor::parse_message(&json) {
        Ok(m) => m,
        Err(_) => return,
    };
    let _ = processor.process_message(parsed);
}

// ─── Driving the chat ────────────────────────────────────────────────────────

/// A subscription source: a background thread that emits a [`Message::Tick`]
/// every ~100 ms. This is the Iced counterpart of the ratatui loop's
/// `event::poll(Duration::from_millis(100))` — the pacing the ratatui version
/// uses to stream one protocol message per tick.
///
/// A plain OS thread + an unbounded mpsc channel is used (rather than
/// `iced::time::every`) for parity with the `17_scifi_hud` example; the thread
/// exits cleanly when the receiver is dropped (i.e. when the app closes).
fn tick_stream() -> iced::futures::channel::mpsc::UnboundedReceiver<Message> {
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if tx.unbounded_send(Message::Tick).is_err() {
                break; // receiver dropped — app closed
            }
        }
    });
    rx
}

fn main() -> iced::Result {
    // Build the function map up front. `ChatApp::new` takes it by value, but
    // Iced's `BootFn` accepts a plain `Fn()` (not `FnOnce`), so we stash the
    // map behind a `Mutex<Option<…>>` the boot closure drains exactly once —
    // the same boot trick `17_scifi_hud`'s main uses to thread built state
    // into the Elm constructor.
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().to_string(), f))
        .collect();
    let boot_cell = std::sync::Mutex::new(Some(functions));

    iced::application(
        move || {
            let functions = boot_cell
                .lock()
                .expect("boot cell poisoned")
                .take()
                .expect("boot cell already drained");
            ChatApp::new(functions)
        },
        ChatApp::update,
        ChatApp::view,
    )
    .title(|_state: &ChatApp| "A2UI · Agent Chat (Iced)".to_string())
    .theme(|_state: &ChatApp| Theme::Dark)
    .subscription(move |_state: &ChatApp| Subscription::run(tick_stream))
    .window_size(iced::Size::new(900.0, 700.0))
    .resizable(true)
    .run()
}
