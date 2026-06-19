//! # Example: A2UI Agent Chat — egui backend
//!
//! The egui counterpart of the ratatui [`08_agent_chat`]: an AI-agent chat
//! window where a mock agent streams A2UI protocol messages and each AI
//! response is a **separate A2UI surface** rendered through the generic
//! a2ui-egui walker. A text input at the bottom sends user messages.
//!
//! ## What it demonstrates
//! - **Multi-surface streaming**: every AI response lives on its own surface
//!   (`welcome`, `msg-1`, `msg-2`, …) and is rendered into the chat list by a
//!   direct call to [`a2ui_egui::walker::render_node`] — the same call the
//!   gallery host uses, just driven per entry instead of per surface.
//! - **Progressive `text/a2ui` streaming**: `createSurface` →
//!   `updateComponents` → `updateDataModel` messages drain one per ~100 ms
//!   tick (throttled on egui's frame clock, mirroring the ratatui loop's
//!   `event::poll` and the Iced/Bevy/Dioxus equivalents).
//! - **egui immediate-mode chat layout**: a `ScrollArea` lists entries, each AI
//!   surface wrapped in a `Frame` for a chat-bubble look; the input row is
//!   pinned below and the list auto-scrolls to the bottom on new content.
//! - **Shared scenario logic**: the welcome message and the per-command
//!   scenarios come from [`a2ui_tui::agent_chat`], so the egui chat renders the
//!   identical content to every other backend's `08_agent_chat`.
//!
//! The only "data source" is the same mock agent every backend uses; this
//! example just feeds its JSON through a [`MessageProcessor`] and asks the
//! walker to draw each resulting surface.
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-egui --example 08_agent_chat --features backend
//! ```
//!
//! ## Controls
//! - Type a message and press `Enter` to send
//! - Available commands: hello, weather, tasks, story, stats, quote, help
//! - `Esc` (or the OS window-close button) to quit
//!
//! [`08_agent_chat`]: ../../../a2ui/examples/08_agent_chat.rs

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Vec2, ViewportCommand};

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_egui::edit_state::EditBuffers;
use a2ui_egui::interaction::PendingInteraction;
use a2ui_egui::walker::render_node;
use a2ui_tui::agent_chat::{generate_response, welcome_messages};
use a2ui_tui::catalogs::basic::build_basic_catalog;

// ─── Chat state ───────────────────────────────────────────────────────────────

/// One row of the conversation: a user message (just text) or an AI message
/// (a surface rendered through the walker).
struct ChatEntry {
    role: String,       // "user" or "ai"
    surface_id: String, // empty for user messages
    text: String,       // user message text
}

// ─── The application ──────────────────────────────────────────────────────────

/// The chat window: a [`MessageProcessor`] owning every AI surface, the live
/// streaming queue, the input field, and the empty walker-side state
/// (`EditBuffers`/`PendingInteraction`/caches) that `render_node` needs.
struct ChatApp {
    processor: MessageProcessor,
    functions: HashMap<String, Box<dyn FunctionImplementation>>,
    entries: Vec<ChatEntry>,
    input: String,
    msg_counter: u32,
    pending_messages: Vec<serde_json::Value>,
    pending_timer: u8,
    typing: bool,
    /// Wall-clock seconds of the last ~100 ms streaming tick.
    last_tick: f64,
    /// Entry count at the last frame — drives bottom-stick auto-scroll.
    last_entry_count: usize,

    // Walker-side state. Chat surfaces are static, so `pending` stays empty and
    // the caches/modals stay empty, but `render_node` still requires them.
    edit_buffers: EditBuffers,
    pending: Vec<PendingInteraction>,
    image_cache: HashMap<String, Option<egui::TextureHandle>>,
    local_tabs: HashMap<String, usize>,
    open_modals: HashSet<String>,
}

impl ChatApp {
    fn new() -> Self {
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

        // Welcome surface: feed every welcome message up front (no streaming for
        // the greeting — it appears fully formed when the window opens), then
        // add its AI entry.
        let welcome_sid = "welcome".to_string();
        for msg in welcome_messages(&welcome_sid) {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            let parsed = MessageProcessor::parse_message(&json).unwrap();
            processor.process_message(parsed).unwrap();
        }

        let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
            .into_iter()
            .map(|f| (f.name().to_string(), f))
            .collect();

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
            last_tick: 0.0,
            last_entry_count: 0,
            edit_buffers: EditBuffers::default(),
            pending: Vec::new(),
            image_cache: HashMap::new(),
            local_tabs: HashMap::new(),
            open_modals: HashSet::new(),
        }
    }

    /// Drain one queued protocol message into the processor (the ~100 ms tick).
    /// A `createSurface` message also pushes a fresh AI chat entry so the new
    /// response gets its own bubble as it streams in.
    fn drain_one(&mut self) {
        if self.pending_timer > 0 {
            self.pending_timer -= 1;
            return;
        }
        let Some(msg) = self.pending_messages.first().cloned() else {
            return;
        };
        let json = serde_json::to_string(&msg).unwrap_or_default();
        if let Ok(parsed) = MessageProcessor::parse_message(&json) {
            let sid = msg
                .get("createSurface")
                .and_then(|cs| cs.get("surfaceId"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(sid) = sid {
                self.entries.push(ChatEntry {
                    role: "ai".into(),
                    surface_id: sid,
                    text: String::new(),
                });
                self.typing = false;
            }

            let _ = self.processor.process_message(parsed);
        }
        self.pending_messages.remove(0);
        self.pending_timer = 1;
    }

    /// Send the current input as a user message (no-op while streaming or empty).
    fn send(&mut self) {
        if !self.pending_messages.is_empty() {
            return;
        }
        let msg = self.input.trim().to_string();
        if msg.is_empty() {
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
        self.pending_timer = 2;
    }
}

impl eframe::App for ChatApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        // Throttle the streaming tick to ~100 ms on egui's frame clock, then
        // keep repainting so the stream continues between input events.
        let now = ctx.input(|i| i.time);
        if now - self.last_tick >= 0.100 {
            self.last_tick = now;
            self.drain_one();
        }
        ctx.request_repaint_after(Duration::from_millis(100));

        // `Esc` quits (window-close works too via the OS button).
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Whole-window layout: scroll list fills, input row pinned to the bottom.
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, Color32::from_rgb(0x12, 0x14, 0x18));
        if full.width() < 20.0 || full.height() < 20.0 {
            return;
        }
        let content = full.shrink(10.0);
        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            let streaming = !self.pending_messages.is_empty();

            // ── Chat list (scrollable) ────────────────────────────────────
            let input_h = 36.0;
            let list_h = (ui.available_height() - input_h).max(40.0);
            ui.allocate_ui(Vec2::new(ui.available_width(), list_h), |list| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(list, |ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        ui.set_min_width(ui.available_width());

                        let entries_len = self.entries.len();
                        for i in 0..entries_len {
                            // Borrow a single entry's fields by index so the
                            // walker (which borrows `self.processor`) can run
                            // without holding an iterator borrow on `entries`.
                            let (role, surface_id, text) = {
                                let e = &self.entries[i];
                                (e.role.clone(), e.surface_id.clone(), e.text.clone())
                            };
                            if role == "user" {
                                ui.label(
                                    RichText::new(format!("👤 You:  {text}"))
                                        .color(Color32::from_rgb(0x56, 0xf0, 0xff)),
                                );
                            } else if role == "ai" {
                                // Render this surface through the generic walker.
                                let Some(surface) =
                                    self.processor.model.get_surface(&surface_id)
                                else {
                                    continue;
                                };
                                let dm = surface.data_model.borrow();
                                let comps = surface.components.borrow();
                                if comps.contains("root") {
                                    Frame::group(ui.style())
                                        .fill(Color32::from_rgb(0x1c, 0x22, 0x2b))
                                        .stroke((1.0, Color32::from_rgb(0x33, 0x3a, 0x44)))
                                        .corner_radius(8.0)
                                        .inner_margin(8.0)
                                        .outer_margin(0.0)
                                        .show(ui, |ui| {
                                            ui.set_min_width(
                                                (ui.available_width() - 20.0).max(1.0),
                                            );
                                            render_node(
                                                "root",
                                                &surface.id,
                                                "",
                                                ui,
                                                &dm,
                                                &comps,
                                                &self.functions,
                                                None,
                                                &self.open_modals,
                                                &self.image_cache,
                                                &self.local_tabs,
                                                &mut self.edit_buffers,
                                                &mut self.pending,
                                            );
                                        });
                                }
                            }
                        }

                        // "Thinking" indicator after the last entry while we wait
                        // for the first streaming message to land.
                        if self.typing && self.pending_messages.is_empty() {
                            ui.label(
                                RichText::new("🤖 AI is thinking …")
                                    .color(Color32::from_rgb(0x88, 0x88, 0x88)),
                            );
                        }
                    });
            });

            // Auto-scroll once when new entries land (beyond ScrollArea's
            // stick-to-bottom, which only tracks the same-content height).
            if self.entries.len() > self.last_entry_count {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
            }
            self.last_entry_count = self.entries.len();

            // ── Input row (pinned below the scroll list) ──────────────────
            ui.allocate_space(Vec2::ZERO);
            ui.horizontal(|row| {
                let resp = row.add_sized(
                    Vec2::new((row.available_width() - 140.0).max(40.0), 22.0),
                    egui::TextEdit::singleline(&mut self.input)
                        .hint_text("Type a message (hello, weather, tasks, story, stats, quote, help)")
                        .interactive(!streaming)
                        .desired_width(f32::MAX),
                );
                if resp.lost_focus()
                    && row.input(|i| i.key_pressed(egui::Key::Enter))
                    && !streaming
                {
                    self.send();
                    resp.request_focus();
                }
                if streaming {
                    row.label(
                        RichText::new("⏳ Streaming…")
                            .color(Color32::from_rgb(0xff, 0xb4, 0x54)),
                    );
                } else {
                    if row.button("Send").clicked() {
                        self.send();
                    }
                }
            });

            // `pending` absorbs any walker interactions; chat surfaces are
            // static, so it is expected to stay empty and is dropped here.
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("A2UI · Agent Chat (egui)"),
        ..Default::default()
    };
    eframe::run_native(
        "A2UI Agent Chat (egui)",
        options,
        Box::new(move |_cc| Ok(Box::new(ChatApp::new()))),
    )
}
