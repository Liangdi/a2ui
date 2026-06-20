//! # Example: A2UI Agent Chat — Bevy backend
//!
//! An AI-agent chat window rebuilt on the a2ui protocol and rendered into a real
//! OS window by Bevy's native ECS UI stack. This is the Bevy counterpart of the
//! ratatui-style [`08_agent_chat`]: same mock agent, same streamed scenarios
//! (welcome / hello / weather / tasks / story / stats / quote / help), same
//! "each AI response is its own A2UI surface" architecture — different renderer.
//!
//! Like [`17_scifi_hud`], this example does **not** drive the a2ui-bevy
//! reconciler. It builds ordinary Bevy UI nodes directly and lets only the
//! **data** flow through the protocol (see the `crates/bevy/Cargo.toml`
//! dev-deps comment: *"the layout IS the Bevy entity tree; only the data flows
//! through the protocol"*). The chat owns a `MessageProcessor` in a `NonSend`
//! resource, a `stream_tick` system drains one pending protocol message per
//! ~100 ms (simulating an SSE stream), and a `rebuild_ui` system walks each
//! AI surface's component tree (Column / Row / Card / Text / Divider — the
//! static subset the chat scenarios use) and spawns a matching Bevy entity
//! subtree. User input is read from `KeyboardInput` events and reflected into a
//! `TextBundle` input bar; Enter sends.
//!
//! ## Multi-surface rendering
//! The ratatui original measures each surface and blits a scrolled slice. Bevy
//! 0.18 UI has no equivalent "render a subtree off-screen and slice it", so we
//! take the simpler correct route: every AI entry renders its full surface
//! subtree (a recursive Bevy entity builder handles Column/Row/Card/Text/
//! Divider), all parented under one scrollable messages container
//! (`Overflow::scroll_y` + `ScrollPosition`). A `rebuild_ui` pass despawns and
//! respawns the whole messages subtree whenever `entries` changes (the chat is
//! small, so the cost is negligible) — simplest correct multi-surface rebuild.
//!
//! ## Scrolling
//! Bevy 0.18's `ui_layout` does not auto-wire the mouse wheel to
//! `ScrollPosition`, and the container does not stick to the bottom on its own.
//! So: the messages container carries a `ScrollPosition`; `rebuild_ui` pins it
//! to the bottom whenever content grows (auto-scroll, matching the ratatui
//! `auto_scroll` flag); ArrowUp/Down/PageUp/PageDown/mouse-wheel adjust the
//! offset away from the bottom (any manual scroll turns auto-scroll off until
//! the user scrolls back to the bottom, exactly like the ratatui original).
//!
//! ## Controls
//! - Type a message and press **Enter** to send (while not streaming).
//! - Available commands: `hello`, `weather`, `tasks`, `story`, `stats`,
//!   `quote`, `help`.
//! - **↑/↓** scroll 3 lines, **PageUp/PageDown** scroll a page, **mouse wheel**
//!   scrolls 3 lines.
//! - `Esc` or window-close to quit.
//!
//! [`08_agent_chat`]: ../../a2ui/examples/08_agent_chat.rs
//! [`17_scifi_hud`]: ./17_scifi_hud.rs
//!
//! ## Run
//! ```sh
//! cargo run -p a2ui-bevy --example 08_agent_chat --features backend
//! ```

use std::time::Duration;

use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::WindowResolution;
use serde_json::Value;

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{ChildList, DynamicString};
use a2ui_tui::agent_chat::{generate_response, welcome_messages};
use a2ui_tui::catalogs::basic::build_basic_catalog;

// ─── Palette ─────────────────────────────────────────────────────────────────
// Bevy's `Color::srgb` takes sRGB 0..1 components and converts to linear.
const BG: Color = Color::srgb(0.050, 0.054, 0.058);
const CARD_BG: Color = Color::srgb(0.110, 0.118, 0.129);
const CARD_BORDER: Color = Color::srgb(0.220, 0.231, 0.247);
const DIVIDER: Color = Color::srgb(0.220, 0.231, 0.247);
const CYAN: Color = Color::srgb(0.337, 0.941, 1.0);
const TEXT: Color = Color::srgb(0.902, 0.914, 0.925);
const DIM: Color = Color::srgb(0.451, 0.466, 0.490);

// ─── Runtime state ───────────────────────────────────────────────────────────

/// One row of the conversation. A user row carries `text`; an AI row carries the
/// `surface_id` of the streamed A2UI surface to render.
struct ChatEntry {
    role: String,       // "user" | "ai"
    surface_id: String, // empty for user messages
    text: String,       // user message text
}

/// The chat's runtime state. Held as a **`NonSend` resource**: `MessageProcessor`
/// contains `RefCell`-backed model maps that are `!Sync`, so it cannot satisfy
/// Bevy's `Send + Sync` resource requirement. Systems take `NonSendMut` /
/// `NonSend` — single-threaded, one at a time.
struct ChatState {
    processor: MessageProcessor,
    entries: Vec<ChatEntry>,
    input: String,
    msg_counter: u32,
    pending_messages: Vec<Value>,
    pending_timer: u8,
    typing: bool,
    /// Set whenever `entries` changes or streaming advances; `rebuild_ui`
    /// consumes it (rebuild + clear). A coarse flag rather than per-field dirty
    /// bits — the chat is small and a full rebuild is cheap.
    dirty: bool,
    /// True while the view should track the newest content. Any manual scroll
    /// (Arrow/Page/wheel) turns this off; scrolling back to the bottom re-enables
    /// it (matches the ratatui original's `auto_scroll` flag).
    auto_scroll: bool,
}

impl ChatState {
    fn new() -> Self {
        let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

        // Seed the welcome surface synchronously (no streaming animation for the
        // very first AI message — same as the ratatui original's boot block).
        let welcome_sid = "welcome".to_string();
        for msg in welcome_messages(&welcome_sid) {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if let Ok(parsed) = MessageProcessor::parse_message(&json) {
                let _ = processor.process_message(parsed);
            }
        }

        let entries = vec![ChatEntry {
            role: "ai".into(),
            surface_id: welcome_sid,
            text: String::new(),
        }];

        Self {
            processor,
            entries,
            input: String::new(),
            msg_counter: 0,
            pending_messages: Vec::new(),
            pending_timer: 0,
            typing: false,
            dirty: true,
            auto_scroll: true,
        }
    }

    fn streaming(&self) -> bool {
        !self.pending_messages.is_empty()
    }
}

/// Repeating ~100 ms timer driving the stream cadence (the Bevy analogue of the
/// ratatui loop's `event::poll(..., Duration::from_millis(100))`).
#[derive(Resource)]
struct StreamTimer(Timer);

// ─── Marker components ───────────────────────────────────────────────────────

/// Tags the scrollable messages container so `rebuild_ui` can despawn its
/// children and so the scroll systems can find it.
#[derive(Component)]
struct MessagesContainer;

/// Tags the input-bar `Text` so `update_input_bar` can refresh it without a
/// full rebuild.
#[derive(Component)]
struct InputBar;

/// Tags the help/status `Text` above the input bar.
#[derive(Component)]
struct HelpLine;

// ─── App wiring ──────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "A2UI Agent Chat (Bevy)".into(),
                resolution: WindowResolution::new(820, 640),
                ..default()
            }),
            ..default()
        }))
        .insert_non_send(ChatState::new())
        .insert_resource(StreamTimer(Timer::new(
            Duration::from_millis(100),
            TimerMode::Repeating,
        )))
        .add_systems(Startup, spawn_layout)
        .add_systems(
            Update,
            (
                stream_tick,
                handle_keyboard,
                handle_mouse_wheel,
                rebuild_ui,
                update_input_bar,
                exit_on_esc,
            )
                .chain(),
        )
        .run();
}

// ─── Build the static layout shell (once) ────────────────────────────────────

/// Spawn the camera + the root column (scrollable messages container above a
/// fixed input bar). The messages container is re-populated each time `entries`
/// changes by `rebuild_ui`; the shell itself never moves.
fn spawn_layout(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands
        .spawn((
            Name::new("Chat Root"),
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(BG),
        ))
        .with_children(|root| {
            // Header.
            root.spawn((
                Text::new("🤖 A2UI Agent Chat"),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(CYAN),
            ));

            // Scrollable messages area (flex_grow so it fills the remaining
            // space above the input bar). `Overflow::scroll_y` + `ScrollPosition`
            // give us a native scroll container; the wheel/arrow systems below
            // drive the offset (Bevy 0.18 does not auto-wire the wheel).
            root.spawn((
                Name::new("Messages"),
                MessagesContainer,
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    width: Val::Percent(100.0),
                    overflow: Overflow::scroll_y(),
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                ScrollPosition::default(),
            ));

            // Help / status line.
            root.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(DIM),
                HelpLine,
            ));

            // Input bar (a TextBundle reflecting `state.input` + a top border).
            root.spawn((
                Node {
                    display: Display::Flex,
                    width: Val::Percent(100.0),
                    border: UiRect::top(Val::Px(1.0)),
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(6.0)),
                    ..default()
                },
                BorderColor::all(DIVIDER),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Text::new("> "),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(CYAN),
                    InputBar,
                ));
            });
        });
}

// ─── Streaming: drain one pending protocol message per ~100 ms tick ──────────

/// The mock agent's "SSE stream": one message per tick. If `pending_timer` is
/// non-zero we wait (adds a beat between the user's send and the first AI
/// message); otherwise we take the front message, feed it (serialize → parse →
/// process), and — for `createSurface` — push a new AI `ChatEntry` so the new
/// surface renders as its own chat bubble. Marks the UI dirty so `rebuild_ui`
/// re-runs. Matches the ratatui original's streaming block.
fn stream_tick(mut state: NonSendMut<ChatState>, time: Res<Time>, mut timer: ResMut<StreamTimer>) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    if state.pending_timer > 0 {
        state.pending_timer -= 1;
        return;
    }

    if state.pending_messages.is_empty() {
        // Stream drained: clear the "thinking…" indicator.
        if state.typing {
            state.typing = false;
            state.dirty = true;
        }
        return;
    }

    let msg = state.pending_messages.remove(0);
    let json = serde_json::to_string(&msg).unwrap_or_default();
    if let Ok(parsed) = MessageProcessor::parse_message(&json) {
        // A `createSurface` opens a new AI chat bubble for this surface.
        let new_sid = msg
            .get("createSurface")
            .and_then(|cs| cs.get("surfaceId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(sid) = new_sid {
            state.entries.push(ChatEntry {
                role: "ai".into(),
                surface_id: sid,
                text: String::new(),
            });
            state.typing = false;
        }
        let _ = state.processor.process_message(parsed);
    }
    state.pending_timer = 1;
    state.dirty = true;
}

// ─── Keyboard input ──────────────────────────────────────────────────────────

/// Read typed characters into `state.input`; Backspace pops; Enter sends (if
/// non-empty and not streaming); Arrow/PageUp/Down adjust the scroll offset.
/// Uses `KeyboardInput` events (Bevy 0.18's `ReceivedCharacter` is deprecated in
/// favor of `Key::Character` on `KeyboardInput`).
fn handle_keyboard(
    mut state: NonSendMut<ChatState>,
    mut events: MessageReader<KeyboardInput>,
    mut scroll_query: Query<&mut ScrollPosition, With<MessagesContainer>>,
) {
    let streaming = state.streaming();

    for ev in events.read() {
        // Only react to key presses (not releases/repeats handled elsewhere).
        if !ev.state.is_pressed() {
            continue;
        }

        match &ev.logical_key {
            // ── Enter: send ───────────────────────────────────────────────
            Key::Enter if !streaming => {
                let msg = state.input.trim().to_string();
                if msg.is_empty() {
                    continue;
                }
                state.input.clear();
                state.entries.push(ChatEntry {
                    role: "user".into(),
                    surface_id: String::new(),
                    text: msg.clone(),
                });
                state.typing = true;
                state.auto_scroll = true;
                state.msg_counter += 1;
                let sid = format!("msg-{}", state.msg_counter);
                state.pending_messages = generate_response(&sid, &msg);
                state.pending_timer = 2;
                state.dirty = true;
            }
            // ── Backspace ─────────────────────────────────────────────────
            Key::Backspace => {
                state.input.pop();
            }
            // ── Typed character ───────────────────────────────────────────
            Key::Character(s) if !streaming => {
                if state.input.len() < 200 {
                    // `Character` carries the full resolved grapheme (e.g. "a",
                    // "A" with shift); `s` is a `SmolStr`, deref to `&str`.
                    state.input.push_str(s.as_str());
                }
            }
            // ── Scroll: Arrow Up / Down ───────────────────────────────────
            Key::ArrowUp => {
                if let Ok(mut sp) = scroll_query.single_mut() {
                    sp.0.y = (sp.0.y + 30.0).max(0.0);
                    state.auto_scroll = false;
                }
            }
            Key::ArrowDown => {
                if let Ok(mut sp) = scroll_query.single_mut() {
                    sp.0.y = (sp.0.y - 30.0).max(0.0);
                    state.auto_scroll = sp.0.y <= 0.5;
                }
            }
            // ── Scroll: PageUp / PageDown ─────────────────────────────────
            Key::PageUp => {
                if let Ok(mut sp) = scroll_query.single_mut() {
                    sp.0.y = (sp.0.y + 200.0).max(0.0);
                    state.auto_scroll = false;
                }
            }
            Key::PageDown => {
                if let Ok(mut sp) = scroll_query.single_mut() {
                    sp.0.y = (sp.0.y - 200.0).max(0.0);
                    state.auto_scroll = sp.0.y <= 0.5;
                }
            }
            _ => {}
        }
    }
}

/// Mouse wheel → scroll the messages container (Bevy 0.18 does not auto-wire
/// wheel events to `ScrollPosition`). Any wheel motion turns auto-scroll off
/// until the user wheels back to the bottom.
fn handle_mouse_wheel(
    mut state: NonSendMut<ChatState>,
    mut wheel: MessageReader<MouseWheel>,
    mut scroll_query: Query<&mut ScrollPosition, With<MessagesContainer>>,
) {
    let mut delta = 0.0f32;
    for ev in wheel.read() {
        // `y` is in lines (pixel-scroll devices report small fractional values;
        // treat 1 unit ≈ 20 px to match the line-scroll feel of Arrow keys).
        delta -= ev.y * 20.0;
    }
    if delta == 0.0 {
        return;
    }
    if let Ok(mut sp) = scroll_query.single_mut() {
        sp.0.y = (sp.0.y + delta).max(0.0);
        state.auto_scroll = sp.0.y <= 0.5;
    }
}

// ─── Rebuild the messages subtree when dirty ─────────────────────────────────

/// Walk `entries`; for each, despawn+respawn the messages subtree. Each AI entry
/// renders its full A2UI surface (Column/Row/Card/Text/Divider) via a recursive
/// Bevy entity builder. After rebuilding, pin the scroll to the bottom if
/// `auto_scroll` is on (so streaming content stays visible). This is the
/// simplest correct multi-surface approach — the chat is small, so the cost of a
/// full rebuild on each streamed tick is negligible.
///
/// The borrow on the data model / component map is scoped to a block and dropped
/// before any spawn commands run (the builder captures owned data, not refs).
fn rebuild_ui(
    mut state: NonSendMut<ChatState>,
    mut commands: Commands,
    messages_query: Query<Entity, With<MessagesContainer>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;

    let Ok(container) = messages_query.single() else {
        return;
    };

    // Despawn existing message children (simplest correct rebuild). `despawn`
    // would also remove the container itself; `despawn_children` leaves the
    // container and recursively removes its subtree.
    commands.entity(container).despawn_children();

    // Snapshot the rows we need to render as owned tuples, so the immutable
    // borrow on `state.processor` (below) is the only one outstanding during the
    // spawn pass. Each row is `(role, surface_id, text)`.
    let rows: Vec<(String, String, String)> = state
        .entries
        .iter()
        .map(|e| (e.role.clone(), e.surface_id.clone(), e.text.clone()))
        .collect();
    let typing = state.typing;
    let streaming = state.streaming();
    let auto_scroll = state.auto_scroll;

    // Borrow the processor immutably for the whole spawn pass; Bevy's
    // `with_children` closure runs inline so the borrow is released when this
    // fn returns.
    let processor = &state.processor;

    commands.entity(container).with_children(|parent| {
        for (role, surface_id, text) in &rows {
            if role == "user" {
                spawn_user_entry(parent, text);
            } else {
                spawn_ai_entry(parent, surface_id, processor);
            }
        }
        // "Thinking…" indicator while the agent is composing before the first
        // message of the response lands.
        if typing && !streaming {
            parent.spawn((
                Text::new("🤖 AI is thinking …"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(DIM),
            ));
        }
    });

    // Pin to the bottom on auto-scroll. Done after spawning so the new content
    // participates in layout. Bevy clamps `ScrollPosition` to the content range
    // during the next layout pass, so setting a large `offset_y` reliably parks
    // the view at the bottom once the freshly-spawned children measure.
    if auto_scroll {
        commands
            .entity(container)
            .insert(ScrollPosition(Vec2::new(0.0, f32::MAX)));
    }
}

/// A user chat bubble: a single cyan line "👤 You: {text}".
fn spawn_user_entry(parent: &mut RelatedSpawnerCommands<ChildOf>, text: &str) {
    parent.spawn((
        Text::new(format!("👤 You:  {text}")),
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        TextColor(CYAN),
    ));
}

/// An AI chat bubble: the full A2UI surface rendered as a Bevy entity subtree
/// (Column / Row / Card / Text / Divider — the static subset the chat scenarios
/// use). If the surface is missing or not yet populated (still streaming in), a
/// placeholder renders instead.
fn spawn_ai_entry(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    surface_id: &str,
    processor: &MessageProcessor,
) {
    let Some(surface) = processor.model.get_surface(surface_id) else {
        parent.spawn((
            Text::new(format!("🤖 [missing surface: {surface_id}]")),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(DIM),
        ));
        return;
    };
    if !surface.has_root() {
        // Surface created but not yet populated (streaming in): show a
        // "composing" placeholder.
        parent.spawn((
            Text::new("🤖 …"),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(DIM),
        ));
        return;
    }

    // Borrow the component map + data model for the duration of the build.
    let components = surface.components.borrow();
    let data_model = surface.data_model.borrow();
    let functions = std::collections::HashMap::new();

    build_subtree(
        parent,
        "root",
        &components,
        &data_model,
        &functions,
        surface_id,
    );
}

/// Recursive Bevy entity builder for the static subset the chat scenarios use:
/// Column / List → vertical flex; Row → horizontal flex; Card → bordered padded
/// panel with its `child`; Text → styled label (resolving `{"path":"/x"}` data
/// bindings + `variant` font sizing); Divider → thin rule. Other kinds fall back
/// to a bracketed label. Recursion is fine in Rust (the chat trees are shallow).
fn build_subtree(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    component_id: &str,
    components: &SurfaceComponentsModel,
    data_model: &DataModel,
    functions: &std::collections::HashMap<
        String,
        Box<dyn a2ui_base::catalog::function_api::FunctionImplementation>,
    >,
    surface_id: &str,
) {
    let Some(model) = components.get(component_id) else {
        return;
    };
    let kind = model.component_type.as_str();

    let ctx = ComponentContext::new(
        component_id.to_string(),
        surface_id.to_string(),
        data_model,
        components,
        functions,
        "", // root-relative; the chat scenarios use absolute paths
        None,
    );

    match kind {
        "Column" | "List" => {
            parent
                .spawn(Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    width: Val::Percent(100.0),
                    row_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|col| {
                    for (cid, base) in child_plan(model, &ctx) {
                        build_subtree_with_base(
                            col, &cid, &base, components, data_model, functions, surface_id,
                        );
                    }
                });
        }
        "Row" => {
            parent
                .spawn(Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    width: Val::Percent(100.0),
                    column_gap: Val::Px(12.0),
                    align_items: AlignItems::FlexStart,
                    ..default()
                })
                .with_children(|row| {
                    for (cid, base) in child_plan(model, &ctx) {
                        build_subtree_with_base(
                            row, &cid, &base, components, data_model, functions, surface_id,
                        );
                    }
                });
        }
        "Card" => {
            let mut cmd = parent.spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    width: Val::Percent(100.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    margin: UiRect::vertical(Val::Px(2.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(CARD_BG),
                BorderColor::all(CARD_BORDER),
            ));
            // Card has a single `child`.
            if let Some(child_id) = model.child() {
                cmd.with_children(|card| {
                    build_subtree_with_base(
                        card, &child_id, "", components, data_model, functions, surface_id,
                    );
                });
            }
        }
        "Text" => {
            let text = resolve_text(model, &ctx);
            let variant = model.get_property::<String>("variant");
            let (size, color) = text_style(&variant);
            parent.spawn((
                Text::new(text),
                TextFont {
                    font_size: FontSize::Px(size),
                    ..default()
                },
                TextColor(color),
            ));
        }
        "Divider" => {
            parent.spawn((
                Node {
                    display: Display::Flex,
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::vertical(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(DIVIDER),
            ));
        }
        _ => {
            parent.spawn((
                Text::new(format!("[{kind}]")),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(DIM),
            ));
        }
    }
}

/// `build_subtree` with a (possibly non-empty) data-context base path — used for
/// template-expanded children. The chat scenarios use only static `children` /
/// `child`, so the base is almost always empty, but we honor the template shape
/// for completeness (mirrors `render.rs::build_child_plan`).
#[allow(clippy::too_many_arguments)]
fn build_subtree_with_base(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    component_id: &str,
    base: &str,
    components: &SurfaceComponentsModel,
    data_model: &DataModel,
    functions: &std::collections::HashMap<
        String,
        Box<dyn a2ui_base::catalog::function_api::FunctionImplementation>,
    >,
    surface_id: &str,
) {
    // For the static chat scenarios the base path is irrelevant (Text bindings
    // are absolute like `/progress_bar`). Delegate to the plain builder; if a
    // future scenario needs template scoping, thread `base` into the
    // ComponentContext here.
    let _ = base;
    build_subtree(
        parent,
        component_id,
        components,
        data_model,
        functions,
        surface_id,
    );
}

/// Resolve a `Text` component's `text` property — literal or `{"path":"/x"}`
/// data binding — through the data model. Mirrors the resolution
/// `render.rs::resolve_fields` does for `text`.
fn resolve_text(model: &ComponentModel, ctx: &ComponentContext) -> String {
    model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default()
}

/// `(font_size, color)` for a Text `variant`. Matches the ratatui original's
/// emphasis mapping: h1/h2/h3 large + bright, body normal, caption small + dim.
fn text_style(variant: &Option<String>) -> (f32, Color) {
    match variant.as_deref() {
        Some("h1") => (26.0, CYAN),
        Some("h2") => (22.0, CYAN),
        Some("h3") => (18.0, TEXT),
        Some("caption") => (12.0, DIM),
        _ => (15.0, TEXT), // body + unknown
    }
}

/// Build the `(child_id, base_path)` plan for a container component — the same
/// three A2UI child shapes `render.rs::build_child_plan` handles. The chat
/// scenarios use only static `children` / single `child`, but we support the
/// template shape too for completeness.
fn child_plan(model: &ComponentModel, ctx: &ComponentContext) -> Vec<(String, String)> {
    let mut plan = Vec::new();
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
    plan
}

// ─── Input bar + help line refresh ───────────────────────────────────────────

/// Reflect `state.input` into the input-bar `Text` and the status into the help
/// line, without a full rebuild. Runs every frame (cheap: two text writes).
fn update_input_bar(
    state: NonSend<ChatState>,
    // Both queries read `&mut Text`; the `Without<…>` clauses make the two
    // result sets provably disjoint so Bevy's access checker (error B0001)
    // accepts two mutable `Text` queries in one system.
    mut input_query: Query<&mut Text, (With<InputBar>, Without<HelpLine>)>,
    mut help_query: Query<&mut Text, (With<HelpLine>, Without<InputBar>)>,
) {
    let streaming = state.streaming();

    if let Ok(mut prompt) = input_query.single_mut() {
        if streaming {
            prompt.0 = "⏳ Streaming…".to_string();
        } else if state.input.is_empty() {
            prompt.0 =
                "> type a message (hello, weather, tasks, story, stats, quote, help)…".to_string();
        } else {
            prompt.0 = format!("> {}█", state.input);
        }
    }

    if let Ok(mut help) = help_query.single_mut() {
        help.0 = if streaming {
            "⏳ Streaming A2UI messages…   (Esc: quit)".to_string()
        } else {
            "Enter: send   ↑↓/PageUp/Down/wheel: scroll   Esc: quit".to_string()
        };
    }
}

// ─── Quit ────────────────────────────────────────────────────────────────────

/// Esc → `AppExit` (window-close also works, via `DefaultPlugins`).
fn exit_on_esc(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
