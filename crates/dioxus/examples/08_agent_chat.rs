//! # Example: A2UI Agent Chat — Dioxus backend
//!
//! The Dioxus counterpart of the ratatui [`08_agent_chat`]: an AI-agent chat
//! window where a mock agent streams A2UI protocol messages and **each AI
//! response is a separate A2UI surface** rendered into the scroll history. A
//! text input at the bottom sends user messages.
//!
//! This is the example that the Dioxus backend's defining trait —
//! **reactive-signals** — was made for. The whole chat lives in a handful of
//! [`Signal`]s at the root:
//!
//! - `processor: Signal<MessageProcessor>` — owns every surface (one per AI
//!   message). Feeding a parsed protocol message to `processor.write()
//!   .process_message(..)` is the *only* mutation path; the write re-renders
//!   every subscriber, so the signal **is the streaming channel** (no Iced
//!   `Message` enum, no ratatui poll loop).
//! - `entries: Signal<Vec<ChatEntry>>` — the conversation log; each AI entry
//!   carries the `surface_id` its bubble renders.
//! - `pending: Signal<Vec<Value>>` — the not-yet-streamed protocol messages.
//! - `input`, `typing`, `msg_counter` — the input field + agent state.
//!
//! A `spawn`ed async loop drains `pending` one message per ~100 ms tick
//! (`tokio::time::sleep`, exactly like [`17_scifi_hud`]); a `createSurface`
//! message pushes a fresh AI `ChatEntry`, and the matching bubble then renders
//! via [`A2uiNodeInSurface`] — the surface-id-aware variant of the recursive
//! node renderer, because the chat has *many* coexisting surfaces where the
//! gallery (and plain [`A2uiNode`]) assumes a single current one.
//!
//! [`08_agent_chat`]: ../../a2ui/examples/08_agent_chat.rs
//! [`17_scifi_hud`]: ./17_scifi_hud.rs
//! [`A2uiNodeInSurface`]: a2ui_dioxus::A2uiNodeInSurface
//! [`A2uiNode`]: a2ui_dioxus::A2uiNode
//! [`Signal`]: dioxus::prelude::Signal
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-dioxus --example 08_agent_chat --features backend
//! ```
//!
//! ## Controls
//! - Type a message and press **Enter** (or click **Send**) to send.
//! - Available commands: `hello`, `weather`, `tasks`, `story`, `stats`,
//!   `quote`, `help`.
//! - The history auto-scrolls to the newest message.
//! - Close the window to quit.

use std::rc::Rc;
use std::time::Duration;

use serde_json::Value;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_dioxus::{A2uiNodeInSurface, STYLESHEET};
use a2ui_tui::agent_chat::{generate_response, welcome_messages};
use a2ui_tui::catalogs::basic::build_basic_catalog;

use dioxus::desktop::tao::dpi::LogicalSize;
use dioxus::desktop::{Config, WindowBuilder};
use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Chat entry — one per message in the conversation. Props must be Clone, and
// the `entries` signal diffs on PartialEq, so derive both.
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct ChatEntry {
    role: String,       // "user" or "ai"
    surface_id: String, // empty for user messages
    text: String,       // user message text (empty for AI entries)
}

// ---------------------------------------------------------------------------
// The reactive chat root
// ---------------------------------------------------------------------------

fn app() -> Element {
    // The processor holds every surface (one per AI message). It is NOT Clone,
    // so it is built here in the root fn (the signal initializer is FnOnce and
    // may consume the freshly-built catalog) and shared via context — the same
    // pattern as the dioxus-gallery main.
    let processor: Signal<MessageProcessor> = use_signal(build_processor);

    // The function map is read-only + non-Clone, so wrap it in Rc once.
    let functions: Rc<Functions> = use_hook(|| Rc::new(build_function_map()));

    // Conversation + agent state. All Copy signal handles; the spawn loop below
    // captures them by value. Declared `mut` because `.set()`/`.write()` take
    // `&mut self`.
    let mut entries: Signal<Vec<ChatEntry>> =
        use_signal(|| vec![ChatEntry { role: "ai".into(), surface_id: "welcome".into(), text: String::new() }]);
    let mut input: Signal<String> = use_signal(String::new);
    let mut pending: Signal<Vec<Value>> = use_signal(Vec::new);
    let mut typing: Signal<bool> = use_signal(|| false);
    let msg_counter: Signal<u32> = use_signal(|| 0);

    // Share the processor + function map with the surface-pinned node renderer
    // (A2uiNodeInSurface reads them via use_context, exactly like A2uiNode).
    use_context_provider(|| processor);
    use_context_provider(|| functions);

    // `A2uiNodeInSurface` (copied from `A2uiNode`) also reads `focused` and the
    // Button-activation callback `on_activate` from context, so provide both.
    // The chat surfaces are static (no Buttons / focus), so a `None` focus and a
    // no-op activator are enough — but the context *must* exist or the node
    // renderer panics with "Could not find context" at first render.
    let focused: Signal<Option<String>> = use_signal(|| None);
    let on_activate: Rc<dyn Fn(String)> =
        use_hook(|| -> Rc<dyn Fn(String)> { Rc::new(|_id: String| {}) });
    use_context_provider(|| focused);
    use_context_provider(|| on_activate.clone());

    // ── Streaming loop ───────────────────────────────────────────────────
    // Drain `pending` one protocol message per ~100 ms tick. A `createSurface`
    // pushes a fresh AI ChatEntry (so its bubble appears before the surface is
    // fully built — the streamed `updateComponents` then fills it in). Writing
    // `processor` re-renders every bubble that reads it.
    use_hook(|| {
        spawn(async move {
            let mut processor = processor;
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;

                // Snapshot pending; if empty, nothing to stream this tick.
                let front = {
                    let p = pending.read();
                    p.first().cloned()
                };
                let Some(msg) = front else { continue; };

                // A createSurface starts a new AI bubble: push the entry first
                // (copy → edit → set, the Signal write idiom).
                let new_sid = msg
                    .get("createSurface")
                    .and_then(|cs| cs.get("surfaceId"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(sid) = new_sid {
                    let mut local = entries.read().clone();
                    local.push(ChatEntry { role: "ai".into(), surface_id: sid, text: String::new() });
                    entries.set(local);
                    typing.set(false);
                }

                // Feed the protocol message: serialize → parse → process.
                let json = serde_json::to_string(&msg).unwrap_or_default();
                if let Ok(parsed) = MessageProcessor::parse_message(&json) {
                    let _ = processor.write().process_message(parsed);
                }

                // Pop the front of pending (copy → drain → set).
                let mut local = pending.read().clone();
                if !local.is_empty() {
                    local.remove(0);
                }
                pending.set(local);
            }
        });
    });

    // ── Send handler ─────────────────────────────────────────────────────
    // Reads `input`; if non-empty and nothing is streaming, pushes a user
    // entry, arms `typing`, and seeds `pending` with the agent's response.
    // Bound at both call sites (Send button `onclick`, field Enter); the
    // closures ignore their event arg and call the shared `do_send` helper.
    let on_send_click = move |_: Event<MouseData>| {
        do_send(entries, input, pending, typing, msg_counter);
    };

    // ── Enter-to-send on the input ───────────────────────────────────────
    let on_keydown = move |e: KeyboardEvent| {
        if e.key() == Key::Enter {
            do_send(entries, input, pending, typing, msg_counter);
        }
    };

    let streaming = !pending.read().is_empty();
    let show_thinking = *typing.read() && pending.read().is_empty();

    // Snapshot entries into owned per-row data so the rsx `for` body never
    // starts with a `let` and each row owns its strings.
    let rows: Vec<(String, String, String)> = {
        let e = entries.read();
        e.iter()
            .map(|entry| (entry.role.clone(), entry.surface_id.clone(), entry.text.clone()))
            .collect()
    };

    rsx! {
        div { class: "chat",
            // ── Header ───────────────────────────────────────────────────
            div { class: "chat__header",
                span { class: "chat__title", "🤖 A2UI Agent Chat" }
                span { class: "spacer" }
                span { class: "chat__status", "{streaming_label(streaming)}" }
            }

            // ── Scrollable history ───────────────────────────────────────
            div { class: "chat__history",
                for (role, surface_id, text) in rows {
                    {render_bubble(role, surface_id, text)}
                }
                if show_thinking {
                    div { class: "msg-thinking", "🤖 AI is thinking …" }
                }
                div { class: "chat__anchor" }
            }

            // ── Input row (pinned at bottom) ─────────────────────────────
            div { class: "chat__input",
                input {
                    class: "chat__field",
                    value: "{input}",
                    placeholder: "Type a message (hello, weather, tasks, story, stats, quote, help)…",
                    disabled: streaming,
                    oninput: move |e| input.set(e.value()),
                    onkeydown: on_keydown,
                }
                button {
                    class: "chat__send btn btn--primary",
                    r#type: "button",
                    disabled: streaming,
                    onclick: on_send_click,
                    "Send"
                }
            }
        }
    }
}

/// Send the current `input`: if non-empty and nothing is streaming, push a
/// user `ChatEntry`, arm `typing`, and seed `pending` with the agent's
/// response. Reads/writes via the shared signals (the Signal write idiom:
/// copy into a local, edit, then `set`). Extracted as a free fn so both the
/// Send button's `onclick` and the field's Enter handler can call it without
/// re-deriving the closure.
fn do_send(
    mut entries: Signal<Vec<ChatEntry>>,
    mut input: Signal<String>,
    mut pending: Signal<Vec<Value>>,
    mut typing: Signal<bool>,
    mut msg_counter: Signal<u32>,
) {
    let msg = input.read().trim().to_string();
    if msg.is_empty() {
        return;
    }
    if !pending.read().is_empty() {
        return; // a response is still streaming — drop the send.
    }
    input.set(String::new());

    let mut local_entries = entries.read().clone();
    local_entries.push(ChatEntry { role: "user".into(), surface_id: String::new(), text: msg.clone() });
    entries.set(local_entries);

    typing.set(true);
    let mut c = msg_counter.write();
    *c += 1;
    let sid = format!("msg-{}", *c);
    drop(c);

    pending.set(generate_response(&sid, &msg));
}

/// Build one chat bubble for a row. USER → a right-aligned plain text line;
/// AI → a left-aligned bubble rendering the surface via `A2uiNodeInSurface`.
/// Returned as an Element so the `for` loop body stays a single expression.
fn render_bubble(role: String, surface_id: String, text: String) -> Element {
    if role == "user" {
        rsx! {
            div { class: "msg-row msg-row--user",
                div { class: "msg-user", "👤 You:  {text}" }
            }
        }
    } else {
        rsx! {
            div { class: "msg-row msg-row--ai",
                div { class: "msg-ai",
                    A2uiNodeInSurface {
                        surface_id: surface_id,
                        id: "root".to_string(),
                        base_path: String::new(),
                    }
                }
            }
        }
    }
}

/// Status-line copy for the header, mirroring the ratatui help text.
fn streaming_label(streaming: bool) -> String {
    if streaming {
        "⏳ Streaming A2UI messages…".to_string()
    } else {
        "Enter: send  ·  close window: quit".to_string()
    }
}

// ---------------------------------------------------------------------------
// Driving the chat
// ---------------------------------------------------------------------------

/// The merged function map shared via context with `ComponentContext`.
type Functions = std::collections::HashMap<String, Box<dyn FunctionImplementation>>;

/// Build the function map keyed by function name (mirrors the dioxus-gallery).
fn build_function_map() -> Functions {
    a2ui_base::catalog::basic_functions::build_basic_functions()
        .into_iter()
        .map(|f| (f.name().to_string(), f))
        .collect()
}

/// Build the runtime: a processor seeded with the basic catalog, then replay
/// the welcome surface's protocol messages so the first AI bubble is populated
/// on mount.
fn build_processor() -> MessageProcessor {
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);
    for msg in welcome_messages("welcome") {
        let json = serde_json::to_string(&msg).unwrap_or_default();
        if let Ok(parsed) = MessageProcessor::parse_message(&json) {
            let _ = processor.process_message(parsed);
        }
    }
    processor
}

// ---------------------------------------------------------------------------
// Stylesheet — the gallery theme (a2ui_dioxus::STYLESHEET) styles the A2UI
// components (Card / Text / Column / …); this appends chat-shell + bubble CSS.
// ---------------------------------------------------------------------------

const CHAT_STYLE: &str = r#"
.chat {
  display: flex; flex-direction: column; height: 100vh;
  background: var(--crust); color: var(--text);
  font: 13px/1.5 -apple-system, "Segoe UI", system-ui, sans-serif;
}
.chat__header {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 18px; background: var(--mantle);
  border-bottom: 1px solid var(--line);
}
.chat__title { font-size: 15px; font-weight: 600; color: var(--text); }
.chat__status { font-size: 11px; color: var(--sub1); }
.spacer { flex: 1; }
.chat__history {
  flex: 1; overflow-y: auto; padding: 16px 18px;
  display: flex; flex-direction: column; gap: 12px;
}
.chat__anchor { height: 1px; }   /* auto-scroll target */

.msg-row { display: flex; width: 100%; }
.msg-row--user { justify-content: flex-end; }
.msg-row--ai   { justify-content: flex-start; }

.msg-user {
  max-width: 72%; padding: 9px 14px; border-radius: 14px;
  background: var(--acc-wash); color: var(--text);
  border: 1px solid rgba(61,214,140,0.25);
}
.msg-ai {
  max-width: 82%; padding: 4px 10px;          /* let the Card breathe inside */
}
.msg-thinking { color: var(--sub1); font-style: italic; padding: 4px 6px; }

.chat__input {
  display: flex; gap: 8px; padding: 12px 18px;
  background: var(--mantle); border-top: 1px solid var(--line);
}
.chat__field {
  flex: 1; font: inherit; color: var(--text);
  padding: 9px 12px; background: var(--base);
  border: 1px solid var(--edge); border-radius: 9px; outline: none;
}
.chat__field:focus { border-color: var(--acc); box-shadow: 0 0 0 1.5px var(--acc); }
.chat__field:disabled { opacity: .5; }
.chat__send:disabled { opacity: .45; cursor: not-allowed; }
"#;

fn main() {
    // Combine the gallery theme (component classes) with this app's chat-shell
    // CSS, then inject the merged sheet into the document head.
    let stylesheet = format!("{STYLESHEET}\n{CHAT_STYLE}");

    dioxus::LaunchBuilder::new()
        .with_cfg(desktop! {
            Config::new()
                .with_window(
                    WindowBuilder::new()
                        .with_title("A2UI · Agent Chat (Dioxus)")
                        .with_inner_size(LogicalSize::new(900.0, 700.0)),
                )
                .with_custom_head(format!("<style>{stylesheet}</style>"))
        })
        .launch(app);
}
