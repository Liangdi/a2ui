//! Per-component-kind Bevy render — spawn + update the entity's components to
//! mirror the resolved A2UI property values.
//!
//! This is the Bevy counterpart of `crates/egui/src/components.rs`. The key
//! difference: egui rebuilds widgets every frame (immediate mode), while Bevy
//! keeps entities alive across frames. So each `apply_*` fn here is an
//! **idempotent updater** — given an existing entity, it re-applies the state
//! components (`Text`, `Checked`, `SliderValue`, `TextInputBuffer`, styling) to
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

use bevy::prelude::*;
use bevy::color::palettes::tailwind::{GRAY_200, GRAY_400, GRAY_800, BLUE_600};
use bevy::ui::Checked;
use bevy::ui_widgets::{Button, Checkbox, Slider, SliderRange, SliderValue};
use bevy_ui_text_input::{TextInputMode, TextInputNode};

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
/// produces in `live_tree.rs`.
pub struct NodeFields {
    pub text: String,
    pub label: String,
    pub checked: bool,
    pub number: f64,
    pub variant: Option<String>,
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
    let _ = kind;
    NodeFields { text, label, checked, number, variant }
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
        TextFont { font_size, ..default() },
        TextColor(Color::from(GRAY_800)),
    ));
}

/// Icon — labeled placeholder (no icon font yet).
pub fn apply_icon(mut cmd: EntityCommands, fields: &NodeFields) {
    cmd.insert((
        Text::new(format!("[icon: {}]", fields.text)),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::from(GRAY_400)),
    ));
}

/// DateTimeInput — label + value placeholder.
pub fn apply_date_time_input(mut cmd: EntityCommands, fields: &NodeFields) {
    cmd.insert((
        Text::new(format!("{}: {}", fields.label, fields.text)),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::from(GRAY_800)),
    ));
}

/// Image / Video / Audio — labeled placeholder.
pub fn apply_media_placeholder(mut cmd: EntityCommands, kind: &str, fields: &NodeFields) {
    cmd.insert((
        Text::new(format!("[{kind}: {}]", fields.text)),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::from(GRAY_400)),
    ));
}

/// ChoicePicker — placeholder label.
pub fn apply_choice_picker(mut cmd: EntityCommands, fields: &NodeFields) {
    cmd.insert((
        Text::new(format!("[ChoicePicker: {}]", fields.label)),
        TextFont { font_size: 14.0, ..default() },
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
    // in Bevy 0.18). Primary fills blue; borderless is transparent; default none.
    let bg = match fields.variant.as_deref() {
        Some("primary") => Some(BackgroundColor(Color::from(BLUE_600))),
        Some("borderless") => Some(BackgroundColor(Color::NONE)),
        _ => None,
    };
    match bg {
        Some(b) => {
            cmd.insert((Button, node, b));
        }
        None => {
            cmd.insert((Button, node));
        }
    }
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

/// TextField — `bevy_ui_text_input::TextInputNode` (single-line). The buffer is
/// seeded from the resolved data-model value on first spawn (via the widget's
/// own `TextInputQueue`, which its `process_text_input_queues` system applies).
/// See [`crate::interaction::collect_text_field_changes`] for the write-back.
pub fn apply_text_field(mut cmd: EntityCommands, fields: &NodeFields, focused: bool) {
    cmd.insert((
        TextInputNode {
            mode: TextInputMode::SingleLine,
            clear_on_submit: false,
            unfocus_on_submit: false,
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
    // The initial buffer seed happens once at spawn, in the reconciler, via a
    // queued `Paste` action on the widget's own `TextInputQueue`.
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
