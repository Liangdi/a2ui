//! Per-component-kind Bevy render — spawn + update the entity's components to
//! mirror the resolved A2UI property values.
//!
//! This is the Bevy counterpart of `crates/egui/src/components.rs`. The key
//! difference: egui rebuilds widgets every frame (immediate mode), while Bevy
//! keeps entities alive across frames. So each `apply_*` fn here is an
//! **idempotent updater** — given an existing entity, it re-applies the state
//! components (`Text`, `Checked`, `SliderValue`, `EditableText`, styling) to
//! match the resolved values. The reconciler ([`crate::reconcile`]) owns
//! spawn/despawn and calls these.
//!
//! `bevy_ui_widgets` widgets are **external-state**: they do not flip their own
//! `Checked`/`SliderValue` (we did not add the `_self_update` observers). So we
//! drive them from the data model every frame, and [`crate::interaction`]
//! reports user interactions back. This is the "data-model as source of truth"
//! contract the egui/slint backends also honor.
//!
//! Property resolution mirrors egui: each component builds a
//! [`ComponentContext`] and resolves `DynamicString`/`DynamicNumber`/
//! `DynamicBoolean` through its [`DataContext`].

use bevy::color::palettes::tailwind::{BLUE_600, GRAY_200, GRAY_400, GRAY_800};
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::Checked;
use bevy::ui_widgets::{Button, Checkbox, Slider, SliderRange, SliderValue};

use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::model::components_model::SurfaceComponentsModel;
use a2ui_base::model::data_model::DataModel;
use a2ui_base::protocol::common_types::{ChildList, DynamicBoolean, DynamicNumber, DynamicString};

use crate::interaction::PendingInteraction;

/// The set of resolved, backend-neutral values a component needs to render.
///
/// Built once per node per frame from the [`ComponentContext`], then passed to
/// the per-kind updater. Mirrors the field-tuple egui's `resolve_fields`
/// produces in `live_tree.rs`. Fields only meaningful to one kind (e.g.
/// `icon_name` for Icon) are left empty for the others — `resolve_fields`
/// populates everything opportunistically and each `apply_*` reads what it needs.
#[derive(Clone)]
pub struct NodeFields {
    pub text: String,
    pub label: String,
    pub checked: bool,
    pub number: f64,
    pub variant: Option<String>,
    /// Resolved `value` `DynamicString` — the editable text for TextField /
    /// DateTimeInput (their value lives under `value`, **not** `text`). Also
    /// used to seed the text-input buffer on first spawn.
    pub value_string: String,
    /// Resolved `name` `DynamicString` — the glyph name for Icon (Icons have no
    /// `text` property; their identity is `name`).
    pub icon_name: String,
    /// Resolved `url` `DynamicString` — the source URL for Image (cache key +
    /// placeholder label).
    pub image_url: String,
    /// Resolved `description` `DynamicString` — the alt text for Image.
    pub image_description: String,
    /// `enableDate` flag for DateTimeInput (picks the format-hint placeholder).
    pub enable_date: bool,
    /// `enableTime` flag for DateTimeInput (picks the format-hint placeholder).
    pub enable_time: bool,
}

impl NodeFields {
    /// An all-empty `NodeFields` — the starting point for synthetic nodes
    /// (tab titles / choice options), which only populate `text` + `label`.
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            label: String::new(),
            checked: false,
            number: 0.0,
            variant: None,
            value_string: String::new(),
            icon_name: String::new(),
            image_url: String::new(),
            image_description: String::new(),
            enable_date: true,
            enable_time: true,
        }
    }
}

/// Resolve the display fields for a component of `kind`, through `ctx`.
pub fn resolve_fields(kind: &str, ctx: &ComponentContext, model: &ComponentModel) -> NodeFields {
    let text = model
        .get_property::<DynamicString>("text")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let checked = model
        .get_property::<DynamicBoolean>("value")
        .map(|db| ctx.data_context.resolve_dynamic_boolean(&db))
        .unwrap_or(false);
    let number = model
        .get_property::<DynamicNumber>("value")
        .map(|dn| ctx.data_context.resolve_dynamic_number(&dn))
        .unwrap_or(0.0);
    let variant: Option<String> = model.get_property("variant");
    // `value` as a string — the editable content for TextField / DateTimeInput.
    let value_string = model
        .get_property::<DynamicString>("value")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    // `name` — the Icon glyph name.
    let icon_name = model
        .get_property::<DynamicString>("name")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    // Image source + alt text.
    let image_url = model
        .get_property::<DynamicString>("url")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let image_description = model
        .get_property::<DynamicString>("description")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    // DateTimeInput format flags (default both on, matching the spec).
    let enable_date: bool = model.get_property("enableDate").unwrap_or(true);
    let enable_time: bool = model.get_property("enableTime").unwrap_or(true);
    let _ = kind;
    NodeFields {
        text,
        label,
        checked,
        number,
        variant,
        value_string,
        icon_name,
        image_url,
        image_description,
        enable_date,
        enable_time,
    }
}

// ===========================================================================
// Containers — layout nodes that parent their children.
// ===========================================================================

/// Column / List — vertical flex stack.
pub fn apply_column(mut cmd: EntityCommands) {
    cmd.insert(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        ..default()
    });
}

/// Row — horizontal flex stack.
pub fn apply_row(mut cmd: EntityCommands) {
    cmd.insert(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        ..default()
    });
}

/// Card — bordered + padded container.
pub fn apply_card(mut cmd: EntityCommands) {
    cmd.insert((
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::from(GRAY_200)),
        BorderColor::all(Color::from(GRAY_400)),
    ));
}

/// Tabs — render the active child; the tab bar itself is deferred (placeholder).
/// Container direction is column so the active panel stacks vertically.
pub fn apply_tabs(mut cmd: EntityCommands) {
    cmd.insert(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        ..default()
    });
}

/// Modal — its trigger renders in-place; content renders under the overlay root
/// (driven by the reconciler). The in-tree Modal node is just an invisible
/// passthrough for its trigger child.
pub fn apply_modal(mut cmd: EntityCommands) {
    cmd.insert(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        ..default()
    });
}

/// Divider — thin horizontal rule.
pub fn apply_divider(mut cmd: EntityCommands) {
    cmd.insert((
        Node {
            display: Display::Flex,
            width: Val::Percent(100.0),
            height: Val::Px(1.0),
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(Color::from(GRAY_400)),
    ));
}

/// Generic flex container used by `unknown` + overlay roots.
pub fn apply_flex_column(mut cmd: EntityCommands) {
    cmd.insert(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Val::Percent(100.0),
        ..default()
    });
}

// ===========================================================================
// Content / leaf
// ===========================================================================

/// Text — styled label; `variant` h1/h2/h3 select larger font sizes.
pub fn apply_text(mut cmd: EntityCommands, fields: &NodeFields) {
    let font_size = match fields.variant.as_deref() {
        Some("h1") => 28.0,
        Some("h2") => 22.0,
        Some("h3") => 18.0,
        _ => 16.0,
    };
    cmd.insert((
        Text::new(fields.text.clone()),
        TextFont {
            font_size: FontSize::Px(font_size),
            ..default()
        },
        TextColor(Color::from(GRAY_800)),
    ));
}

/// Icon — renders the emoji glyph for the resolved `name`, drawn in the
/// embedded emoji icon font (`A2uiState::icon_font`). Bevy's bundled default
/// font (`FiraMono-subset`) covers almost no icon glyphs, so every Icon uses
/// this dedicated font. The mapping ([`map_icon_emoji`]) is Bevy-specific (emoji
/// codepoints) but covers the same logical names the TUI / Iced backends do;
/// unknown names fall back to `[xx]` (matching the other backends' fallback).
pub fn apply_icon(mut cmd: EntityCommands, fields: &NodeFields, icon_font: &Handle<Font>) {
    let glyph = map_icon_emoji(&fields.icon_name);
    cmd.insert((
        Text::new(glyph),
        TextFont {
            font: icon_font.clone().into(),
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::from(GRAY_800)),
    ));
}

/// Map an A2UI icon name to an emoji / unicode glyph. Mirrors the TUI / Iced
/// backends' logical icon set, but uses **emoji codepoints** (📦 ✉ ➤ …) so a
/// single emoji font (the embedded `a2ui-icons.ttf`) covers every glyph —
/// Bevy's default font has none of them. Unknown names fall back to the first
/// two characters in brackets, matching the TUI / Iced `map_icon` fallback.
fn map_icon_emoji(name: &str) -> String {
    let glyph = match name {
        "mail" => "📧",
        "send" => "📤",
        "search" => "🔍",
        "settings" => "⚙",
        "star" => "⭐",
        "accountCircle" | "person" => "👤",
        "home" => "🏠",
        "heart" | "favorite" => "❤",
        "check" => "✅",
        "close" => "❌",
        "add" => "➕",
        "remove" => "➖",
        "edit" => "✏",
        "delete" => "🗑",
        "refresh" => "🔄",
        "arrowBack" => "⬅",
        "arrowForward" => "➡",
        "arrowUp" | "up" => "⬆",
        "arrowDown" | "down" => "⬇",
        "info" => "ℹ",
        "warning" => "⚠",
        "error" => "⛔",
        "success" => "✅",
        "calendarToday" => "📅",
        "locationOn" => "📍",
        "payment" => "💳",
        "phone" => "📞",
        "play" => "▶",
        "pause" => "⏸",
        "stop" => "⏹",
        "skipNext" | "next" => "⏭",
        "skipPrevious" | "previous" => "⏮",
        _ => return format!("[{}]", name.chars().take(2).collect::<String>()),
    };
    glyph.to_string()
}

/// DateTimeInput — a native, editable `EditableText` bound to `value` (reusing
/// the TextField chrome). Bevy ships no calendar/clock widget, so the value
/// is an editable ISO text field: the user types the ISO string and edits write
/// back to the data model — a genuinely interactive control, not the read-only
/// label this backend showed before. `enableDate` / `enableTime` are not exposed
/// as a placeholder hint (the external text-input widget has no placeholder
/// field), but the resolved value is seeded on first spawn like a TextField.
pub fn apply_date_time_input(mut cmd: EntityCommands, _fields: &NodeFields, focused: bool) {
    cmd.insert((
        EditableText {
            allow_newlines: false,
            ..default()
        },
        Node {
            display: Display::Flex,
            width: Val::Px(220.0),
            height: Val::Px(28.0),
            padding: UiRect::horizontal(Val::Px(6.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(if focused {
            Color::from(BLUE_600)
        } else {
            Color::from(GRAY_400)
        }),
    ));
}

/// Image — a real decoded raster via a native `ImageNode`, or a labeled
/// placeholder Text while the image is still loading / on decode failure.
///
/// This is an **idempotent updater** run every frame: when a `Handle` is
/// available it strips any stale placeholder `Text` components and inserts the
/// `ImageNode` (+ a size-constraining `Node`); when not, it does the inverse.
/// Because `apply_kind` re-runs each frame on the persistent entity, the
/// placeholder→image swap happens exactly once (when the cache populates) and
/// re-inserting the same handle thereafter is a no-op.
pub fn apply_image(mut cmd: EntityCommands, fields: &NodeFields, handle: Option<&Handle<Image>>) {
    match handle {
        Some(h) => {
            cmd.remove::<Text>()
                .remove::<TextFont>()
                .remove::<TextColor>();
            cmd.insert((
                Node {
                    display: Display::Flex,
                    width: Val::Px(320.0),
                    height: Val::Auto,
                    max_width: Val::Percent(100.0),
                    ..default()
                },
                ImageNode {
                    image: h.clone(),
                    ..default()
                },
            ));
        }
        None => {
            cmd.remove::<ImageNode>();
            let label = if fields.image_description.is_empty() {
                "image"
            } else {
                &fields.image_description
            };
            cmd.insert((
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Text::new(format!("[🖼 {}]", label)),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::from(GRAY_400)),
            ));
        }
    }
}

/// Video / Audio — labeled placeholder. Bevy has no media playback widgets (and
/// no video decode pipeline for UI), so these stay placeholders — matching every
/// native backend except the Dioxus WebView.
pub fn apply_media_placeholder(mut cmd: EntityCommands, kind: &str, fields: &NodeFields) {
    cmd.insert((
        Text::new(format!("[{kind}: {}]", fields.image_url)),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::from(GRAY_400)),
    ));
}

/// The synthetic Tabs **tab bar** — a horizontal Row container that parents the
/// clickable title buttons. Emitted by the reconciler for each Tabs component.
pub fn apply_tab_bar(mut cmd: EntityCommands) {
    cmd.insert(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(2.0),
        ..default()
    });
}

/// The synthetic **tab title** button — a clickable `Button` whose own `Text`
/// is the title, highlighted when active. The reconciler attaches the
/// [`crate::state::TabTitle`] marker (carrying the write-back pointer) on the
/// same entity so the button observer routes the click to a tab activation.
pub fn apply_tab_title(mut cmd: EntityCommands, fields: &NodeFields, active: bool) {
    let bg = if active {
        BackgroundColor(Color::from(BLUE_600))
    } else {
        BackgroundColor(Color::from(GRAY_200))
    };
    let color = if active {
        TextColor(Color::WHITE)
    } else {
        TextColor(Color::from(GRAY_800))
    };
    cmd.insert((
        Button,
        Node {
            display: Display::Flex,
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            ..default()
        },
        bg,
        Text::new(fields.text.clone()),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        color,
    ));
}

/// The synthetic **choice option** button — a clickable `Button` whose `Text`
/// shows a `●`/`○` (single) or `☑`/`☐` (multi) prefix + the label, highlighted
/// when selected. The reconciler attaches the [`crate::state::ChoiceOption`]
/// marker (carrying the write-back pointer + value) on the same entity so the
/// button observer routes the click to a single/multi selection.
pub fn apply_choice_option(mut cmd: EntityCommands, fields: &NodeFields, selected: bool) {
    let color = if selected {
        TextColor(Color::from(BLUE_600))
    } else {
        TextColor(Color::from(GRAY_800))
    };
    cmd.insert((
        Button,
        Node {
            display: Display::Flex,
            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
            ..default()
        },
        Text::new(fields.text.clone()),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        color,
    ));
}

// ===========================================================================
// Modal overlay chrome (synthetic — wraps an open Modal's content subtree)
// ===========================================================================
//
// An open Modal is rendered as: a dimmed full-window scrim (click-to-dismiss)
// centering a bordered panel that holds a title + close-button header above the
// Modal's `content` subtree — mirroring the Iced/egui backends. The reconciler
// spawns these synthetic nodes around the content (see `reconcile::plan_tree`).
// Default Bevy picking *blocks* lower nodes, so clicks on the panel/content hit
// them, not the scrim behind — only clicks outside the panel dismiss the Modal.

/// The synthetic scrim — a full-window semi-transparent backdrop that centers
/// its child panel. It is a `Button` carrying the [`ModalDismiss`] marker so a
/// click on the exposed backdrop area dismisses the Modal.
pub fn apply_modal_scrim(mut cmd: EntityCommands) {
    cmd.insert((
        Button,
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.55)),
    ));
}

/// The synthetic panel — a bordered card that holds the header + content.
pub fn apply_modal_panel(mut cmd: EntityCommands) {
    cmd.insert((
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Val::Px(440.0),
            max_width: Val::Percent(90.0),
            padding: UiRect::all(Val::Px(16.0)),
            row_gap: Val::Px(12.0),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::from(GRAY_200)),
        BorderColor::all(Color::from(GRAY_400)),
    ));
}

/// The synthetic header row — title on the left, close button on the right.
pub fn apply_modal_header(mut cmd: EntityCommands) {
    cmd.insert(Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        width: Val::Percent(100.0),
        column_gap: Val::Px(12.0),
        ..default()
    });
}

/// The synthetic title text (the Modal's `title` property, or "Dialog").
pub fn apply_modal_title(mut cmd: EntityCommands, fields: &NodeFields) {
    cmd.insert((
        Text::new(fields.text.clone()),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::from(GRAY_800)),
    ));
}

/// The synthetic close button (✕) — a `Button` carrying the [`ModalDismiss`]
/// marker so a click closes the Modal.
pub fn apply_modal_close(mut cmd: EntityCommands, fields: &NodeFields) {
    cmd.insert((
        Button,
        Node {
            display: Display::Flex,
            padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::from(GRAY_400)),
        Text::new(fields.text.clone()),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::from(GRAY_800)),
    ));
}

// ===========================================================================
// Interactive (native Bevy widgets)
// ===========================================================================

/// Button — a `bevy_ui_widgets::Button` whose child is a `Text` label. The
/// press → `Activate` event is collected by [`crate::interaction::collect_button_activate`].
pub fn apply_button(mut cmd: EntityCommands, fields: &NodeFields) {
    let node = Node {
        display: Display::Flex,
        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
        margin: UiRect::vertical(Val::Px(2.0)),
        ..default()
    };
    // Background is a separate `BackgroundColor` component (Node has no bg field
    // in Bevy 0.18). Primary fills blue; borderless is transparent; the default
    // (secondary) gets a subtle gray fill + border so it reads as a button
    // rather than bare label text.
    let (bg, border) = match fields.variant.as_deref() {
        Some("primary") => (
            Some(BackgroundColor(Color::from(BLUE_600))),
            BorderColor::all(Color::from(BLUE_600)),
        ),
        Some("borderless") => (
            Some(BackgroundColor(Color::NONE)),
            BorderColor::all(Color::NONE),
        ),
        _ => (
            Some(BackgroundColor(Color::from(GRAY_200))),
            BorderColor::all(Color::from(GRAY_400)),
        ),
    };
    // bg is always Some now (default/primary/borderless each set one); keep the
    // Option for clarity. Border is always set.
    cmd.insert((Button, node, bg.unwrap(), border));
    // The label is a child Text node; the reconciler parents it (its `child`
    // is the A2UI Text component). So we only style the button shell here.
}

/// CheckBox — `bevy_ui_widgets::Checkbox` whose `Checked` state mirrors the
/// resolved data-model value (external state). Toggle → `ValueChange<bool>`
/// collected by [`crate::interaction::collect_checkbox_change`].
pub fn apply_checkbox(mut cmd: EntityCommands, fields: &NodeFields) {
    if fields.checked {
        cmd.insert((Checkbox, Checked));
    } else {
        cmd.insert(Checkbox);
    }
    // The label renders as a sibling Text node under the same container — the
    // reconciler wires the A2UI component tree, so we keep the widget headless.
    let _ = fields.label.clone();
}

/// Slider — `bevy_ui_widgets::Slider` with `SliderValue` mirroring the resolved
/// value (external state). Drag → `ValueChange<f32>` collected by
/// [`crate::interaction::collect_slider_change`].
pub fn apply_slider(mut cmd: EntityCommands, fields: &NodeFields) {
    cmd.insert((
        Slider::default(),
        SliderValue(fields.number as f32),
        SliderRange::new(0.0, 100.0),
        Node {
            display: Display::Flex,
            width: Val::Px(200.0),
            height: Val::Px(24.0),
            ..default()
        },
    ));
}

/// TextField — Bevy's first-party `EditableText` (single-line). The buffer is
/// seeded from the resolved data-model value on first spawn (the reconciler
/// writes it directly via `EditableText::editor_mut().set_text`).
/// See [`crate::interaction::collect_text_field_changes`] for the write-back.
pub fn apply_text_field(mut cmd: EntityCommands, fields: &NodeFields, focused: bool) {
    cmd.insert((
        EditableText {
            allow_newlines: false,
            ..default()
        },
        Node {
            display: Display::Flex,
            width: Val::Px(220.0),
            height: Val::Px(28.0),
            padding: UiRect::horizontal(Val::Px(6.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(if focused {
            Color::from(BLUE_600)
        } else {
            Color::from(GRAY_400)
        }),
    ));
    let _ = (fields.label.clone(), focused);
    // The initial buffer seed happens once at spawn, in the reconciler, via
    // `EditableText::editor_mut().set_text` (see `reconcile::apply_node`).
}

// ===========================================================================
// Child planning — shared with egui/slint (the three A2UI child shapes)
// ===========================================================================

/// Plan a node's children as `(child_id, child_base_path)` pairs, honoring all
/// three A2UI child shapes (`child`, static `children`, template `children`).
///
/// Ported verbatim from `crates/egui/src/components.rs::build_child_plan` (which
/// mirrors `crates/slint/src/live_tree.rs::build_child_plan`). Modal is handled
/// by its own path (trigger in-place; content as overlay), so it is excluded
/// here — the reconciler special-cases Modal.
pub fn build_child_plan(model: &ComponentModel, ctx: &ComponentContext) -> Vec<(String, String)> {
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
            if let Some(serde_json::Value::Array(arr)) = ctx.data_context.get(&path) {
                for i in 0..arr.len() {
                    plan.push((component_id.clone(), format!("{path}/{i}")));
                }
            }
        }
        None => {}
    }
    plan
}

// Quiet unused-import noise for types referenced only in doc links / future use.
#[allow(unused_imports)]
use crate::state::A2uiNode;
#[allow(unused_imports)]
use a2ui_base::catalog::function_api::FunctionImplementation;
#[allow(dead_code)]
fn _types_used(_: &SurfaceComponentsModel, _: &DataModel, _: &PendingInteraction) {}

#[cfg(test)]
mod tests {
    use super::map_icon_emoji;

    #[test]
    fn map_icon_emoji_known_name() {
        assert_eq!(map_icon_emoji("mail"), "📧");
        assert_eq!(map_icon_emoji("star"), "⭐");
        assert_eq!(map_icon_emoji("settings"), "⚙");
        assert_eq!(map_icon_emoji("calendarToday"), "📅");
    }

    #[test]
    fn map_icon_emoji_aliases() {
        // `person` aliases to the same glyph as `accountCircle`.
        assert_eq!(map_icon_emoji("person"), map_icon_emoji("accountCircle"));
        assert_eq!(map_icon_emoji("next"), map_icon_emoji("skipNext"));
    }

    #[test]
    fn map_icon_emoji_unknown_falls_back_to_bracketed_prefix() {
        assert_eq!(map_icon_emoji("XYZ"), "[XY]");
        assert_eq!(map_icon_emoji("k"), "[k]");
    }
}
