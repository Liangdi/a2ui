//! # Example: A2UI Agent Chat — GPUI backend
//!
//! An AI-agent chat window rebuilt on the a2ui protocol, rendered into a real
//! OS window by the GPUI backend. This is the GPUI counterpart of the ratatui
//! [`08_agent_chat`] and the iced `08_agent_chat`: same mock agent, same
//! scenarios (shared via [`a2ui_tui::agent_chat`]), same per-message surface
//! model — different renderer.
//!
//! A mock agent streams A2UI protocol messages (simulating `text/a2ui` SSE).
//! Each AI response is a **separate** a2ui surface: a `createSurface` message
//! opens it, then `updateComponents` / `updateDataModel` messages populate it.
//! Every AI chat entry is rendered through a small read-only walker defined in
//! this file (the chat scenarios only emit static `Text` / `Card` / `Column` /
//! `Row` / `Divider` — no interactive widgets — so it needs no state bridge).
//!
//! ## What it demonstrates
//! - Multiple surfaces (one per AI message) rendered in a chat layout.
//! - Progressive A2UI streaming driven by a GPUI background timer: a
//!   [`gpui::Timer`] fires every ~100 ms inside a `cx.spawn` task, and each
//!   tick feeds one pending protocol message to the processor
//!   (`createSurface` opens a new chat entry; the rest populate it).
//! - A gpui-component `Input` composer (with [`InputState`]) bound to Enter,
//!   disabled while a response is streaming.
//! - Chat-bubble styling and auto-scroll to the newest entry via a
//!   [`gpui::ScrollHandle`].
//!
//! [`08_agent_chat`]: ../a2ui/examples/08_agent_chat.rs
//! [`InputState`]: gpui_component::input::InputState
//!
//! ## Run
//! ```sh
//! cargo run --manifest-path crates/gpui/Cargo.toml --example 08_agent_chat --features backend
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
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{ChildList, DynamicString};
use a2ui_tui::agent_chat::{generate_response, welcome_messages};
use a2ui_tui::catalogs::basic::build_basic_catalog;

use gpui::prelude::*;
use gpui::{
    AnyElement, Application, Context, Entity, Render, ScrollHandle, Subscription, Timer, TitlebarOptions,
    WindowBounds, WindowOptions, px, size,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::divider::Divider;
use gpui_component::TitleBar;
use gpui_component::{Root, h_flex, init as init_component, v_flex};
use gpui_component_assets::Assets;
use serde_json::Value;

// ─── Palette (mirrors the iced chat example) ─────────────────────────────────

/// Compose an opaque [`gpui::Rgba`] from 8-bit RGB (gpui's own `rgb` is a
/// non-const runtime fn, so a const fn is needed for `const` palette items).
const fn rgb(r: u8, g: u8, b: u8) -> gpui::Rgba {
    gpui::Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

const USER_ACCENT: gpui::Rgba = rgb(0x56, 0xF0, 0xFF);
const AI_ACCENT: gpui::Rgba = rgb(0x5D, 0xFF, 0xB0);
const DIM: gpui::Rgba = rgb(0x80, 0x8C, 0x99);
const TEXT: gpui::Rgba = rgb(0xEB, 0xF0, 0xF7);
const BG: gpui::Rgba = rgb(0x1A, 0x1C, 0x24);
const BUBBLE: gpui::Rgba = rgb(0x29, 0x2C, 0x38);

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

/// The chat's runtime state.
struct ChatApp {
    processor: MessageProcessor,
    functions: HashMap<String, Box<dyn FunctionImplementation>>,
    entries: Vec<ChatEntry>,
    input_state: Entity<InputState>,
    /// Mirror of the input field's text (the source of truth is `InputState`;
    /// this cache is read in `render` without borrowing the entity).
    input_text: String,
    msg_counter: u32,
    /// The simulated SSE queue: protocol messages not yet fed to the processor.
    pending_messages: Vec<Value>,
    /// Ticks to wait before feeding the next pending message (pacing).
    pending_timer: u8,
    /// True while we are waiting for the first `createSurface` of a response.
    typing: bool,
    /// Chat scroll container handle, for pin-to-bottom auto-scroll.
    scroll_handle: ScrollHandle,
    /// Whether to pin the view to the newest entry this frame.
    pin_to_bottom: bool,
    /// Keep the InputState subscriptions alive.
    _subs: Vec<Subscription>,
}

impl ChatApp {
    /// Boot: build the processor + function map, seed the welcome surface,
    /// construct the input, wire its events, and start the streaming tick.
    fn new(window: &mut gpui::Window, cx: &mut Context<Self>) -> Self {
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
        let welcome_sid = "welcome".to_string();
        for msg in welcome_messages(&welcome_sid) {
            feed(&mut processor, &msg);
        }

        let input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Type a message (hello, weather, tasks, story, stats, quote, help)…"));

        // PressEnter → send; Change → mirror the text into `input_text`.
        let sub = cx.subscribe_in(&input_state, window, move |this, _src, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {
                    this.input_text = this.input_state.read(cx).value().to_string();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.send(Some(window), cx);
                    cx.notify();
                }
                _ => {}
            }
        });

        // Streaming tick: one protocol message per ~100 ms. The task exits when
        // the view is dropped (the WeakEntity upgrade fails).
        cx.spawn(async move |view, cx| {
            loop {
                Timer::after(Duration::from_millis(100)).await;
                let Ok(_) = view.update(cx, |this, cx| {
                    this.step_stream();
                    cx.notify();
                }) else {
                    break;
                };
            }
        })
        .detach();

        Self {
            processor,
            functions: build_basic_functions()
                .into_iter()
                .map(|f| (f.name().to_string(), f))
                .collect(),
            entries: vec![ChatEntry {
                role: "ai".into(),
                surface_id: welcome_sid,
                text: String::new(),
            }],
            input_state,
            input_text: String::new(),
            msg_counter: 0,
            pending_messages: Vec::new(),
            pending_timer: 0,
            typing: false,
            scroll_handle: ScrollHandle::new(),
            pin_to_bottom: true,
            _subs: vec![sub],
        }
    }

    /// Advance the simulated stream by one protocol message (one per tick).
    fn step_stream(&mut self) {
        if self.pending_timer > 0 {
            self.pending_timer -= 1;
            return;
        }
        let Some(msg) = self.pending_messages.first().cloned() else {
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
            self.pin_to_bottom = true;
        }

        feed(&mut self.processor, &msg);
        self.pending_messages.remove(0);
        self.pending_timer = 1; // pace: one message, then a one-tick pause
        self.pin_to_bottom = true;
    }

    /// Send the current input: push a user entry, pick a scenario, prime the
    /// pending queue, and clear the composer.
    fn send(&mut self, window: Option<&mut gpui::Window>, cx: &mut Context<Self>) {
        let msg = self.input_text.trim().to_string();
        if msg.is_empty() || !self.pending_messages.is_empty() || self.typing {
            return;
        }

        // Clear the composer (needs a Window for `set_value`).
        if let Some(window) = window {
            self.input_state
                .update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.input_text.clear();

        self.entries.push(ChatEntry {
            role: "user".into(),
            surface_id: String::new(),
            text: msg.clone(),
        });

        self.typing = true;
        self.msg_counter += 1;
        let sid = format!("msg-{}", self.msg_counter);
        self.pending_messages = generate_response(&sid, &msg);
        self.pending_timer = 2; // brief pause so the "thinking" indicator shows
        self.pin_to_bottom = true;
    }

    /// Render the chat: a scrollable column of bubbles over a fixed input row.
    fn render_chat(&self, cx: &mut Context<Self>) -> AnyElement {
        let streaming = !self.pending_messages.is_empty() || self.typing;

        let mut list = v_flex().gap_3().w_full();
        for entry in &self.entries {
            list = list.child(match entry.role.as_str() {
                "user" => bubble(self.user_bubble(&entry.text), USER_ACCENT, true),
                _ => bubble(self.ai_bubble(entry, cx), AI_ACCENT, false),
            });
        }
        if self.typing {
            list = list.child(bubble(
                gpui::div()
                    .text_color(DIM)
                    .text_size(px(13.))
                    .child("🤖 AI is thinking …")
                    .into_any_element(),
                AI_ACCENT,
                false,
            ));
        }

        // Pin to the newest entry while streaming / on growth.
        if self.pin_to_bottom {
            self.scroll_handle.scroll_to_item(usize::MAX);
        }

        let chat = gpui::div()
            .id("chat-scroll")
            .track_scroll(&self.scroll_handle)
            .overflow_y_scroll()
            .size_full()
            .child(list);

        // Input row.
        let status = if streaming {
            gpui::div()
                .text_color(DIM)
                .text_size(px(11.))
                .child("🤖 Streaming A2UI messages…")
        } else {
            gpui::div()
                .text_color(DIM)
                .text_size(px(11.))
                .child("Enter: send   ·   close window: quit")
        };
        let composer = v_flex()
            .gap_1()
            .w_full()
            .child(Input::new(&self.input_state))
            .child(status);

        v_flex()
            .size_full()
            .child(gpui::div().flex_1().min_h_0().px_4().pt_3().child(chat))
            .child(gpui::div().w_full().px_4().pt_2().pb_3().child(composer))
            .into_any_element()
    }

    /// Render an AI entry: its surface's `root` component through the read-only
    /// walker, wrapped in a bubble.
    fn ai_bubble(&self, entry: &ChatEntry, _cx: &mut Context<Self>) -> AnyElement {
        match self.processor.model.get_surface(&entry.surface_id) {
            Some(surface) => render_surface_root(surface, &self.functions),
            None => gpui::div().text_color(DIM).child("(surface not ready)").into_any_element(),
        }
    }

    /// A user message: a one-line label, right-aligned, in the user accent.
    fn user_bubble(&self, text_content: &str) -> AnyElement {
        gpui::div()
            .text_color(TEXT)
            .text_size(px(14.))
            .child(format!("👤 You:  {text_content}"))
            .into_any_element()
    }
}

impl Render for ChatApp {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.render_chat(cx);
        // `pin_to_bottom` was consumed in `render_chat`; reset until the next
        // growth / streamed message re-sets it.
        self.pin_to_bottom = false;
        v_flex()
            .size_full()
            .bg(BG)
            .child(
                TitleBar::new().child(
                    gpui::div()
                        .text_color(DIM)
                        .text_size(px(12.))
                        .child("A2UI · Agent Chat (GPUI)"),
                ),
            )
            .child(gpui::div().flex_1().min_h_0().child(body))
    }
}

// ─── Bubble chrome ───────────────────────────────────────────────────────────

/// Wrap content in a rounded bubble; `right` pushes it to the right edge (user),
/// else it sits left (AI). A thin accent stripe distinguishes the two.
fn bubble(content: AnyElement, accent: gpui::Rgba, right: bool) -> AnyElement {
    let body = gpui::div()
        .bg(BUBBLE)
        .border_1()
        .border_color(accent)
        .rounded_lg()
        .p_3()
        .max_w(px(720.))
        .child(content);
    // A fill spacer on one side pushes the bubble to the opposite edge.
    let spacer = gpui::div().flex_1();
    if right {
        h_flex().w_full().child(spacer).child(body).into_any_element()
    } else {
        h_flex().w_full().child(body).child(spacer).into_any_element()
    }
}

// ─── Read-only surface walker (chat scenarios are static) ────────────────────

/// Render a surface's `root` component into a read-only GPUI element tree.
/// Handles the static kinds the agent_chat scenarios emit (`Column`, `Row`,
/// `Card`, `Text`, `Divider`); unknown kinds show a muted placeholder.
fn render_surface_root(
    surface: &a2ui_base::model::surface_model::SurfaceModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
) -> AnyElement {
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();
    if !components.contains("root") {
        return gpui::div().text_color(DIM).child("…").into_any_element();
    }
    render_node_ro("root", &surface.id, "", &data_model, &components, functions)
}

/// One read-only recursion step. No interaction handlers (chat surfaces are
/// static), so no `cx` / state caches are needed — unlike the gallery walker.
fn render_node_ro(
    id: &str,
    surface_id: &str,
    base_path: &str,
    data_model: &DataModel,
    components: &SurfaceComponentsModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
) -> AnyElement {
    let Some(model) = components.get(id) else {
        return gpui::div().into_any_element();
    };
    let ctx = ComponentContext::new(
        id.to_string(),
        surface_id.to_string(),
        data_model,
        components,
        functions,
        base_path,
        None,
    );
    match model.component_type.as_str() {
        "Column" => v_flex().gap_2().w_full().children(children_ro(model, &ctx, surface_id, data_model, components, functions)),
        "Row" => h_flex().gap_2().children(children_ro(model, &ctx, surface_id, data_model, components, functions)),
        "Card" => gpui::div()
            .bg(BUBBLE)
            .border_1()
            .border_color(DIM)
            .rounded_lg()
            .p_3()
            .w_full()
            .child(v_flex().gap_2p5().children(children_ro(model, &ctx, surface_id, data_model, components, functions))),
        "Text" => render_text_ro(&ctx, model),
        "Divider" => gpui::div().child(Divider::horizontal().color(DIM)),
        _ => gpui::div()
            .text_color(DIM)
            .text_size(px(12.))
            .child(format!("{} · unknown", model.component_type)),
    }
    .into_any_element()
}

/// Resolve + render a `Text` component with its `variant` heading size. Returns
/// a `Div` (not `AnyElement`) so it type-matches the other arms of the
/// `render_node_ro` match, whose result is then `.into_any_element()`-ed once.
fn render_text_ro(ctx: &ComponentContext, model: &ComponentModel) -> gpui::Div {
    let content = model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let variant: Option<String> = model.get_property("variant");
    let mut el = gpui::div().text_color(TEXT).child(content);
    el = match variant.as_deref() {
        Some("h1") => el.text_size(px(24.)),
        Some("h2") => el.text_size(px(20.)),
        Some("h3") => el.text_size(px(16.)),
        Some("caption") => el.text_color(DIM).text_size(px(12.)),
        _ => el.text_size(px(14.)),
    };
    el
}

/// Build the child elements of a container (handles `child`, static `children`,
/// and template `children`), mirroring the gallery walker's `build_child_plan`.
#[allow(clippy::too_many_arguments)]
fn children_ro(
    model: &ComponentModel,
    ctx: &ComponentContext,
    surface_id: &str,
    data_model: &DataModel,
    components: &SurfaceComponentsModel,
    functions: &HashMap<String, Box<dyn FunctionImplementation>>,
) -> Vec<AnyElement> {
    let mut plan: Vec<(String, String)> = Vec::new();
    let base = ctx.data_context.base_path().to_string();
    if let Some(child_id) = model.child() {
        plan.push((child_id, base.clone()));
    }
    match model.children() {
        Some(ChildList::Static(ids)) => {
            for cid in ids {
                plan.push((cid.clone(), base.clone()));
            }
        }
        Some(ChildList::Template { component_id, path }) => {
            if let Some(Value::Array(arr)) = ctx.data_context.get(&path) {
                for i in 0..arr.len() {
                    plan.push((component_id.clone(), format!("{path}/{i}")));
                }
            }
        }
        None => {}
    }
    plan.into_iter()
        .map(|(cid, base)| render_node_ro(&cid, surface_id, &base, data_model, components, functions))
        .collect()
}

// ─── Feeding the processor ───────────────────────────────────────────────────

/// Serialize → parse → process one protocol message.
fn feed(processor: &mut MessageProcessor, value: &Value) {
    let Ok(json) = serde_json::to_string(value) else {
        return;
    };
    let Ok(parsed) = MessageProcessor::parse_message(&json) else {
        return;
    };
    let _ = processor.process_message(parsed);
}

// ─── Driving the chat ────────────────────────────────────────────────────────

fn main() {
    Application::new()
        .with_assets(Assets)
        .run(move |cx| {
            init_component(cx);

            let window_options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(900.), px(700.)), cx)),
                titlebar: Some(TitlebarOptions {
                    title: Some("A2UI · Agent Chat (GPUI)".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                // GNOME Wayland ignores server-side decoration requests; force
                // CSD so the TitleBar below draws drag + min/max/close.
                window_decorations: Some(gpui::WindowDecorations::Client),
                ..Default::default()
            };

            cx.spawn(async move |cx| {
                cx.open_window(window_options, |window, cx| {
                    let view = cx.new(|cx| ChatApp::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                })?;
                Ok::<_, anyhow::Error>(())
            })
            .detach();
        });
}
