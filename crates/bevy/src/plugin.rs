//! `A2uiPlugin` — registers the render-loop systems + resources, the widget
//! interaction observers, and spawns the base UI (camera, root container, the
//! sample-browser panel + the surface pane + the overlay root).
//!
//! System ordering (each frame):
//! 1. `collect_button_activate` / `collect_checkbox_change` / `collect_slider_change`
//!    observers fire on widget events; `collect_text_field_changes` system
//!    polls TextField buffers. All push to `PendingInteractions`.
//! 2. `apply_interactions_full` consumes the queue, mutates `A2uiState`.
//! 3. `reconcile` diff/patches the entity tree to match the model.
//!
//! The plugin pulls in the widget plugins: `bevy_ui_widgets::UiWidgetsPlugins`
//! (registers Button/Checkbox/Slider observers) and
//! `bevy_ui_text_input::TextInputPlugin`. The host app supplies
//! `DefaultPlugins` (which carries `UiPlugin`, windowing, picking, render).

use bevy::ecs::prelude::*;
use bevy::prelude::*;

use crate::interaction::{apply_interactions_full, collect_button_activate, collect_checkbox_change,
    collect_slider_change, collect_text_field_changes};
use crate::reconcile::reconcile;
use crate::state::{A2uiState, PendingInteractions};

/// The Bevy plugin. Add to an `App` that already has `DefaultPlugins`, after
/// inserting an [`A2uiState`] resource.
pub struct A2uiPlugin;

impl Plugin for A2uiPlugin {
    fn build(&self, app: &mut App) {
        // Widget runtimes: the headless widget observers + the text-input widget.
        app.add_plugins(bevy::ui_widgets::UiWidgetsPlugins);
        app.add_plugins(bevy_ui_text_input::TextInputPlugin);

        // Resources (NonSend — see `state.rs`: the processor is !Sync). The host
        // inserts `A2uiState` via `insert_non_send_resource`; we init the queue
        // here so the observers can write to it before `apply_interactions_full`.
        if app.world().get_non_send_resource::<PendingInteractions>().is_none() {
            app.insert_non_send_resource(PendingInteractions::default());
        }

        // Interaction-collection observers — fire on widget events, push to the
        // queue via DeferredWorld.
        app.add_observer(collect_button_activate)
            .add_observer(collect_checkbox_change)
            .add_observer(collect_slider_change)
            .add_observer(crate::sample_browser::on_sample_row_click);

        // The render-loop systems, ordered: collect (text-input poll) → apply →
        // reconcile → update the sample browser. Observers run reactively
        // (outside this chain) but their queue writes land before
        // `apply_interactions_full` because observers for events triggered
        // during Update run within the same Update tick.
        app.add_systems(
            Update,
            (
                collect_text_field_changes,
                apply_interactions_full,
                reconcile,
                crate::sample_browser::update_browser,
            )
                .chain(),
        );
        // Mouse-wheel → browser scroll (independent of the chain). Bevy 0.18's UI
        // does not auto-wire wheel events to ScrollPosition.
        app.add_systems(Update, crate::sample_browser::browser_mouse_wheel);

        // Build the base UI shell once: camera + a root flex row with the
        // sample-browser panel on the left and the surface pane on the right,
        // plus a top-level overlay root for Modal content.
        app.add_systems(Startup, setup_base_ui);
    }
}

/// Base UI: camera + root layout + the two panes + overlay. Stores the
/// surface/overlay root entities into `A2uiState` so the reconciler can parent
/// nodes under them.
fn setup_base_ui(mut commands: Commands, mut state: NonSendMut<A2uiState>) {
    // UI camera.
    commands.spawn(Camera2d);

    // Sample-browser panel (left) — built first so we know its entity.
    let browser = commands
        .spawn((
            Name::new("A2UI Sample Browser"),
            crate::sample_browser::BrowserRoot,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(30.0),
                height: Val::Percent(100.0),
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
            // `ScrollPosition` is required for `Overflow::scroll_y` to track the
            // offset; a MouseWheel system (added below) drives it. Without it the
            // long sample list is clipped and the lower samples are unreachable.
            ScrollPosition::default(),
            BackgroundColor(Color::srgb(0.95, 0.95, 0.97)),
        ))
        .id();

    // Surface pane (right) — the reconciler parents the rendered A2UI tree here.
    let surface_root = commands
        .spawn((
            Name::new("A2UI Surface"),
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                height: Val::Percent(100.0),
                overflow: Overflow::clip(),
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .id();

    // Root row: browser | surface, as the top-level UI container. Parent the two
    // panes after spawning the row (avoid the `children!` macro, which expects
    // spawn-bundles not existing-entity refs in this position).
    commands
        .spawn((
            Name::new("A2UI Root"),
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .add_child(browser)
        .add_child(surface_root);

    // Overlay root (parents open-Modal content; absolutely positioned, on top).
    let overlay_root = commands
        .spawn((
            Name::new("A2UI Overlay"),
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                position_type: PositionType::Absolute,
                left: Val::Percent(30.0),
                top: Val::Px(40.0),
                ..default()
            },
            ZIndex(100),
        ))
        .id();

    state.surface_root = Some(surface_root);
    state.overlay_root = Some(overlay_root);
    state.dirty = true;
}
