//! # Example: A2UI Agent Chat — Slint backend
//!
//! The Slint counterpart of the ratatui [`08_agent_chat`]: an AI-agent chat
//! window where a mock agent streams A2UI protocol messages (a simulated
//! `text/a2ui` SSE stream). Each AI response is a SEPARATE A2UI surface, and a
//! text input at the bottom sends user messages. The same scenarios (greeting,
//! weather, tasks, story, stats, quote, help) are reused from the shared
//! [`a2ui_tui::agent_chat`] module — only the renderer differs.
//!
//! ## Why a flat row list (the defining constraint)
//!
//! Slint **cannot recurse**: a `.slint` struct can't contain itself and a
//! component can't reference itself (slint-ui/slint#4218). The gallery's
//! [`live_tree`] works around this by flattening ONE surface into a bounded-depth
//! `Node0..N` flat array fed to a single `Surface` window. But a chat is MANY
//! surfaces stacked in a scrollable list — there is no built-in way to compose
//! many arbitrary-depth surface trees in one Slint `for` loop.
//!
//! Fortunately the chat scenario surfaces only use **static** components
//! (`Column`, `Row`, `Card`, `Text`, `Divider` — no interactive widgets). So
//! instead of composing N surface trees, we flatten each AI surface into a flat
//! list of styled rows with a plain Rust recursive walker (recursion in Rust is
//! fine; only the Slint *markup* can't recurse), and render the whole chat as
//! ONE scrolling Slint list of `ChatRow`s.
//!
//! Each `ChatRow` carries `kind` (`UserText` / `SurfaceText` / `Divider` /
//! `Boundary`), `text`, `variant` (`h1`/`h2`/`h3`/`body`/`caption`), `indent`,
//! and `is-card`. Cards become a bordered, padded, indented run of rows; Dividers
//! become a thin rule; Text rows are styled by variant. This loses the exact
//! Card/Row nesting geometry of the ratatui version but faithfully shows every
//! `Text`/`Divider` the agent streams, styled by variant — which is the
//! meaningful content of the chat.
//!
//! Rows are pushed into a `VecModel<ChatRow>` and re-applied each tick; Slint
//! repaints reactively (no per-frame rebuild).
//!
//! ## Behavior parity with the ratatui original
//! - Boot: welcome surface streamed in, flattened into rows.
//! - Streaming: a `slint::Timer` (100 ms, `Repeated`) feeds one pending protocol
//!   message per tick (`createSurface` → `updateComponents` → `updateDataModel`),
//!   re-flattening the affected AI surface's rows so streaming updates show live.
//! - Send: the LineEdit's `accepted` callback → push a user row, kick off
//!   `generate_response`, disable input while streaming.
//! - Auto-scroll: best-effort. The chat lives in a Slint `Flickable`; the
//!   programmatic scroll-to-bottom path (driving `viewport-y` from Rust) needs
//!   reliable post-layout content-height feedback, which Slint's inline
//!   `slint!` component doesn't expose cleanly, so the viewport is left
//!   user-scrollable (drag/scroll to see history) rather than auto-pinned.
//!   This is the one notable behavioral deviation from the ratatui original,
//!   which auto-scrolls on new content.
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-slint --example 08_agent_chat --features backend
//! ```
//!
//! ## Controls
//! - Type a message and press **Enter** (or click **Send**) to send.
//! - Available commands: `hello`, `weather`, `tasks`, `story`, `stats`, `quote`, `help`.
//! - Close the window (OS window-close button) to quit. (Slint 1.16's key-event
//!   API is unstable, so Esc isn't bound — same as the `17_scifi_hud` example.)
//! - The chat area is a Slint `Flickable`: drag/scroll to see older messages.
//!   Auto-scroll-to-bottom is best-effort (see `apply_rows` note).
//!
//! [`08_agent_chat`]: ../../a2ui/examples/08_agent_chat.rs
//! [`live_tree`]: ../src/live_tree.rs
//! [`a2ui_tui::agent_chat`]: a2ui_tui::agent_chat

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use serde_json::Value;

use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::surface_model::SurfaceModel;
use a2ui_base::protocol::common_types::{ChildList, DynamicString};
use a2ui_tui::agent_chat::{generate_response, welcome_messages};
use a2ui_tui::catalogs::basic::build_basic_catalog;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

// ─── The chat UI (inline .slint) ──────────────────────────────────────────────
//
// One scrolling list of `ChatRow`s. `kind` selects the row's chrome: a divider
// rule, a user message (cyan, right-leaning), a surface text (styled by variant),
// or a boundary spacer. Cards are an `is-card` flag rendered as a bordered,
// padded rectangle wrapping the row text. The whole chat lives in a `Flickable`
// so we can drive `viewport-y` to keep the bottom visible.

slint::slint! {
    import { LineEdit, Button } from "std-widgets.slint";

    // One flattened chat row.
    struct ChatRow {
        kind: string,       // "user" | "text" | "divider" | "boundary"
        text: string,
        variant: string,    // "h1"|"h2"|"h3"|"body"|"caption" (Text rows only)
        indent: int,        // left indent in px (Card nesting)
        is-card: bool,      // render inside a bordered card rectangle
    }

    // Renders one row's content by kind. A plain component (not recursive) that
    // takes the row as an `in` property and uses `if` to pick the right chrome.
    // Declared before `Chat` so it can be instantiated there.
    component row-text inherits VerticalLayout {
        in property <ChatRow> data;
        // Divider: a thin horizontal rule.
        if data.kind == "divider" : Rectangle {
            height: 1px;
            background: #2a3340;
        }
        // Boundary spacer: a small empty gap between messages.
        if data.kind == "boundary" : Rectangle { height: 6px; }
        // User message: cyan, body-sized.
        if data.kind == "user" : Text {
            text: data.text;
            color: #79c0ff;
            font-size: 14px;
            wrap: word-wrap;
        }
        // Surface text: styled by variant.
        if data.kind == "text" : Text {
            text: data.text;
            color: data.variant == "caption" ? #6e7681 :
                   data.variant == "h1" ? #f0f6fc :
                   data.variant == "h2" ? #f0f6fc :
                   data.variant == "h3" ? #c9d1d9 : #d0d7de;
            font-size: data.variant == "h1" ? 22px :
                       data.variant == "h2" ? 18px :
                       data.variant == "h3" ? 15px :
                       data.variant == "caption" ? 11px : 14px;
            font-weight: data.variant == "h1" ? 800 :
                          data.variant == "h2" ? 700 :
                          data.variant == "h3" ? 600 : 400;
            wrap: word-wrap;
        }
    }

    export component Chat inherits Window {
        title: "A2UI Agent Chat";
        preferred-width: 720px;
        preferred-height: 720px;
        background: #0e1116;

        in property <[ChatRow]> rows: [];
        in property <string> hint: "Enter: send  -  window-close: quit";
        in property <bool> streaming: false;
        // Two-way bound to the LineEdit so Rust can read + clear it.
        in-out property <string> draft: "";

        callback send(string);

        VerticalLayout {
            // ── Chat scroll area ────────────────────────────────────────────
            Flickable {
                vertical-stretch: 1;

                VerticalLayout {
                    padding: 12px;
                    spacing: 4px;

                    for row[i] in root.rows : VerticalLayout {
                        // Each row is its own VerticalLayout so the Flickable's
                        // content stack measures each row's natural height.
                        HorizontalLayout {
                            padding-left: 8px + row.indent * 1px;
                            padding-right: 8px;
                            padding-top: row.kind == "boundary" ? 4px : 2px;
                            padding-bottom: 2px;

                            // Card chrome wraps the row's text (bordered,
                            // padded, indented). Rendered only for card rows.
                            if row.is-card : Rectangle {
                                border-width: 1px;
                                border-color: #2a3340;
                                border-radius: 6px;
                                background: #161b22;
                                VerticalLayout {
                                    padding: 8px;
                                    spacing: 2px;
                                    row-text { data: row; }
                                }
                            }
                            // Non-card row: render the content directly.
                            if !row.is-card : VerticalLayout {
                                padding: 0px;
                                row-text { data: row; }
                            }
                        }
                    }

                    // Empty model needs a non-zero preferred size so Flickable
                    // can lay out before the first row arrives.
                    if root.rows.length == 0 : Text {
                        text: " ";
                        font-size: 12px;
                    }
                }
            }

            // ── Divider between chat and input ──────────────────────────────
            Rectangle {
                height: 1px;
                background: #2a3340;
            }

            // ── Input + hint bar ────────────────────────────────────────────
            VerticalLayout {
                padding: 8px;
                spacing: 4px;
                Text {
                    text: root.hint;
                    color: root.streaming ? #e3b341 : #5b6473;
                    font-size: 11px;
                }
                HorizontalLayout {
                    spacing: 8px;
                    LineEdit {
                        horizontal-stretch: 1;
                        placeholder-text: "Type a message (hello, weather, tasks, story, stats, quote, help)...";
                        text: root.draft;
                        enabled: !root.streaming;
                        edited(text) => { root.draft = text; }
                        accepted => { root.send(root.draft); }
                    }
                    Button {
                        text: "Send";
                        enabled: !root.streaming;
                        clicked => { root.send(root.draft); }
                    }
                }
            }
        }
    }
}

// ─── Runtime state ───────────────────────────────────────────────────────────

/// One conversation turn (user message or an AI surface). User entries carry
/// the message text; AI entries carry their surface id (flattened into rows on
/// each tick).
struct ChatEntry {
    role: Role,
    /// For AI entries: the surface id to flatten. Empty for user entries.
    surface_id: String,
    /// For user entries: the message text.
    text: String,
}

#[derive(PartialEq)]
enum Role {
    User,
    Ai,
}

/// All mutable chat state, owned behind `Rc<RefCell>` so the Slint timer and
/// `send` callback (both closures) can reach it. Slint is single-threaded, so
/// `Rc`/`RefCell` suffice.
struct ChatState {
    processor: MessageProcessor,
    entries: Vec<ChatEntry>,
    msg_counter: u32,
    pending: Vec<Value>,
    /// Countdown ticks before the next pending message is fed (paces streaming).
    pending_timer: u8,
    /// True while an AI response is mid-stream (input disabled).
    streaming: bool,
}

impl ChatState {
    fn new() -> Self {
        let processor = MessageProcessor::new(vec![build_basic_catalog()]);

        let mut state = ChatState {
            processor,
            entries: Vec::new(),
            msg_counter: 0,
            pending: Vec::new(),
            pending_timer: 0,
            streaming: false,
        };

        // Seed the welcome surface and push its AI entry.
        let sid = "welcome".to_string();
        for msg in welcome_messages(&sid) {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if let Ok(parsed) = MessageProcessor::parse_message(&json) {
                let _ = state.processor.process_message(parsed);
            }
        }
        state.entries.push(ChatEntry {
            role: Role::Ai,
            surface_id: sid,
            text: String::new(),
        });

        state
    }

    /// Begin a user turn: push the user row, request a response.
    fn send(&mut self, msg: &str) {
        let msg = msg.trim();
        if msg.is_empty() || self.streaming {
            return;
        }
        self.msg_counter += 1;
        let sid = format!("msg-{}", self.msg_counter);
        self.entries.push(ChatEntry {
            role: Role::User,
            surface_id: String::new(),
            text: msg.to_string(),
        });
        self.pending = generate_response(&sid, msg);
        self.pending_timer = 2;
        self.streaming = true;
    }

    /// Advance the stream by one tick: feed one pending protocol message, then
    /// flag whether a new AI surface was created (so the caller appends its
    /// entry). Returns `true` if any state changed and the rows should refresh.
    fn tick(&mut self) -> bool {
        if self.pending_timer > 0 {
            self.pending_timer -= 1;
            return false;
        }
        let Some(msg) = self.pending.first().cloned() else {
            // Queue drained: stop streaming.
            if self.streaming {
                self.streaming = false;
                return true;
            }
            return false;
        };

        // A createSurface starts a new AI turn — push a fresh entry.
        let new_surface = msg
            .get("createSurface")
            .and_then(|cs| cs.get("surfaceId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(sid) = new_surface {
            self.entries.push(ChatEntry {
                role: Role::Ai,
                surface_id: sid,
                text: String::new(),
            });
        }

        let json = serde_json::to_string(&msg).unwrap_or_default();
        if let Ok(parsed) = MessageProcessor::parse_message(&json) {
            let _ = self.processor.process_message(parsed);
        }
        self.pending.remove(0);
        self.pending_timer = 1;
        true
    }
}

// ─── Flattening: surface component tree → Vec<ChatRow> ───────────────────────
//
// A plain Rust recursive walker (recursion is fine here — only Slint *markup*
// can't recurse). Resolves every `Text` node's text (literal or data binding)
// and variant, emits `Divider` rows, recurses through `Column`/`Row`/`Card`
// (Cards bump the indent and set `is_card` on their subtree). Rows are stacked
// in document order, which is exactly the chat's reading order.

/// Flatten the AI entry's surface into chat rows. Empty if the surface isn't
/// ready yet (no root).
fn flatten_surface(state: &ChatState, entry: &ChatEntry) -> Vec<ChatRow> {
    let Some(surface) = state.processor.model.get_surface(&entry.surface_id) else {
        return Vec::new();
    };
    let data_model = surface.data_model.borrow();
    let components = surface.components.borrow();
    let functions: std::collections::HashMap<String, Box<dyn FunctionImplementation>> =
        std::collections::HashMap::new();

    let mut rows = Vec::new();
    if components.contains("root") {
        flatten_node(
            "root",
            &mut rows,
            0,
            false,
            surface,
            &data_model,
            &components,
            &functions,
        );
    }
    rows
}

/// Resolve a `Text` component's text: a literal string, or a `{"path":"/x"}`
/// data binding read from the surface's data model.
#[allow(clippy::too_many_arguments)]
fn flatten_node(
    id: &str,
    rows: &mut Vec<ChatRow>,
    indent: i32,
    in_card: bool,
    surface: &SurfaceModel,
    data_model: &a2ui_base::model::data_model::DataModel,
    components: &a2ui_base::model::components_model::SurfaceComponentsModel,
    functions: &std::collections::HashMap<String, Box<dyn FunctionImplementation>>,
) {
    let Some(model) = components.get(id) else {
        return;
    };
    let kind = model.component_type.clone();

    // Build a context to resolve data bindings, then plan children — both as
    // owned data so the recursive calls don't fight the borrow.
    let (text, variant, child_ids, child_kind) = {
        let ctx = ComponentContext::new(
            id.to_string(),
            surface.id.clone(),
            data_model,
            components,
            functions,
            "",
            None,
        );
        let text = model
            .get_property::<DynamicString>("text")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
            .unwrap_or_default();
        let variant: String = model.get_property::<String>("variant").unwrap_or_default();

        let mut ids: Vec<String> = Vec::new();
        if let Some(child) = model.child() {
            ids.push(child);
        }
        match model.children() {
            Some(ChildList::Static(list)) => ids.extend(list),
            Some(ChildList::Template { component_id, path }) => {
                if let Some(Value::Array(arr)) = ctx.data_context.get(&path) {
                    for i in 0..arr.len() {
                        ids.push(format!("{component_id}#{path}/{i}"));
                    }
                }
            }
            None => {}
        }
        // A Row's children are rendered stacked (one per line) — see module doc.
        let ck = kind.clone();
        (text, variant, ids, ck)
    };

    match child_kind.as_str() {
        "Text" => {
            rows.push(make_text_row(&text, &variant, indent, in_card));
        }
        "Divider" => {
            rows.push(ChatRow {
                kind: "divider".into(),
                text: "".into(),
                variant: "".into(),
                indent,
                is_card: in_card,
            });
        }
        "Column" | "List" | "Row" => {
            for cid in &child_ids {
                // Template children are encoded as "componentId#path"; split them.
                if let Some((comp, path)) = cid.split_once('#') {
                    flatten_template(
                        comp,
                        path,
                        rows,
                        indent,
                        in_card,
                        surface,
                        data_model,
                        components,
                        functions,
                    );
                } else {
                    flatten_node(
                        cid,
                        rows,
                        indent,
                        in_card,
                        surface,
                        data_model,
                        components,
                        functions,
                    );
                }
            }
        }
        "Card" => {
            // Card's single child becomes an indented, card-styled run.
            for cid in &child_ids {
                flatten_node(
                    cid,
                    rows,
                    indent + 12,
                    true,
                    surface,
                    data_model,
                    components,
                    functions,
                );
            }
        }
        _ => {
            // Unknown leaf: show its text/label if any, else skip.
            if !text.is_empty() {
                rows.push(make_text_row(&text, &variant, indent, in_card));
            }
        }
    }
}

/// Flatten a template child instance (one element of a bound data array), using
/// `path` as the base data context path.
#[allow(clippy::too_many_arguments)]
fn flatten_template(
    id: &str,
    path: &str,
    rows: &mut Vec<ChatRow>,
    indent: i32,
    in_card: bool,
    surface: &SurfaceModel,
    data_model: &a2ui_base::model::data_model::DataModel,
    components: &a2ui_base::model::components_model::SurfaceComponentsModel,
    functions: &std::collections::HashMap<String, Box<dyn FunctionImplementation>>,
) {
    let Some(model) = components.get(id) else {
        return;
    };
    let kind = model.component_type.clone();
    let ctx = ComponentContext::new(
        id.to_string(),
        surface.id.clone(),
        data_model,
        components,
        functions,
        path,
        None,
    );
    let text = model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let variant: String = model.get_property::<String>("variant").unwrap_or_default();
    if kind == "Text" {
        rows.push(make_text_row(&text, &variant, indent, in_card));
    }
}

/// Build a styled text row.
fn make_text_row(text: &str, variant: &str, indent: i32, is_card: bool) -> ChatRow {
    ChatRow {
        kind: "text".into(),
        text: text.into(),
        variant: variant.into(),
        indent,
        is_card,
    }
}

// ─── The "render": rebuild the flat row list and apply it to the window ───────

/// Re-flatten every entry into a fresh `Vec<ChatRow>` and push it into the
/// window's `rows` model. Chat is small, so re-flattening all entries each tick
/// is cheap and keeps streaming updates live without row-range bookkeeping.
fn apply_rows(chat: &Chat, state: &ChatState) {
    let mut out: Vec<ChatRow> = Vec::new();
    for entry in &state.entries {
        match entry.role {
            Role::User => {
                let label = format!("You:  {}", entry.text);
                out.push(ChatRow {
                    kind: "user".into(),
                    text: label.into(),
                    variant: "".into(),
                    indent: 0,
                    is_card: false,
                });
                // A boundary spacer after each message.
                out.push(ChatRow {
                    kind: "boundary".into(),
                    text: "".into(),
                    variant: "".into(),
                    indent: 0,
                    is_card: false,
                });
            }
            Role::Ai => {
                out.extend(flatten_surface(state, entry));
                out.push(ChatRow {
                    kind: "boundary".into(),
                    text: "".into(),
                    variant: "".into(),
                    indent: 0,
                    is_card: false,
                });
            }
        }
    }

    // Swap the whole model in one shot.
    chat.set_rows(ModelRc::new(Rc::new(VecModel::from(out))));
    chat.set_streaming(state.streaming);
    chat.set_hint(if state.streaming {
        "Streaming A2UI messages...".into()
    } else {
        "Enter: send  -  window-close: quit".into()
    });
}

// ─── main ─────────────────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = Rc::new(RefCell::new(ChatState::new()));
    let chat = Chat::new()?;

    // First frame.
    apply_rows(&chat, &state.borrow());

    // Send callback: the LineEdit `accepted` + Send button both fire `send`.
    {
        let s = Rc::clone(&state);
        let weak = chat.as_weak();
        chat.on_send(move |draft: SharedString| {
            let msg = draft.to_string();
            if msg.trim().is_empty() {
                return;
            }
            {
                let mut st = s.borrow_mut();
                if st.streaming {
                    return;
                }
                st.send(&msg);
            }
            // Clear the draft + refresh.
            if let Some(chat) = weak.upgrade() {
                chat.set_draft("".into());
                apply_rows(&chat, &s.borrow());
            }
        });
    }

    // Quit: the OS window-close button hides the window and ends the event
    // loop. (Slint 1.16's key-event API is unstable, so Esc isn't bound here —
    // close the window to quit, as the 17_scifi_hud example does.)

    // The ~100 ms streaming tick — the Slint equivalent of the ratatui loop's
    // `event::poll`. Repeating: feed one pending protocol message, then re-apply
    // the rows so streaming updates show live.
    let timer = Timer::default();
    {
        let s = Rc::clone(&state);
        let weak = chat.as_weak();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let changed = s.borrow_mut().tick();
            if changed && let Some(chat) = weak.upgrade() {
                apply_rows(&chat, &s.borrow());
            }
        });
    }

    // Run the Slint event loop. The timer stays armed for the whole run.
    chat.run()?;
    Ok(())
}
