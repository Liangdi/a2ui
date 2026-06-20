//! Sample browser — the left-hand list of available samples. Clicking a row
//! switches the loaded sample (reset processor, replay messages, clear modals,
//! mark the tree dirty).
//!
//! Bevy-side counterpart of egui's left-hand `SidePanel` + `selectable_label`.
//! Rebuilt by [`update_browser`] whenever the row count drifts from the sample
//! count; clicks handled by [`on_sample_row_click`] (a picking observer).

use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;

use crate::state::A2uiState;

/// Marker on the browser container entity, so [`update_browser`] can find it.
#[derive(Component)]
pub struct BrowserRoot;

/// Marker on each sample-row entity, carrying its sample index.
#[derive(Component)]
pub struct SampleRow(pub usize);

/// Rebuild the sample-browser rows when the row count drifts from the sample
/// count (i.e. on first build or after a sample list change). Selection
/// highlight is applied at rebuild; live re-selection triggers a full rebuild
/// via the dirty flag the click handler sets.
pub fn update_browser(
    mut commands: Commands,
    browser: Single<Entity, With<BrowserRoot>>,
    children: Query<&Children>,
    rows: Query<&SampleRow>,
    mut state: NonSendMut<A2uiState>,
) {
    let root = *browser;
    let selection = state.selected_sample;
    // Rebuild when the row count drifts OR the selection changed since the last
    // rebuild (so the highlight follows clicks — otherwise the rows are static
    // and never update their background/text color).
    let needs_rebuild = state.browser_last_selection != Some(selection);
    // Count existing rows under the browser root.
    let mut existing: Vec<Entity> = Vec::new();
    for c in children.iter() {
        for child in c.iter() {
            if rows.get(child).is_ok() {
                existing.push(child);
            }
        }
    }
    if existing.len() == state.samples.len() && !needs_rebuild {
        return;
    }

    for e in &existing {
        commands.entity(*e).despawn();
    }
    state.browser_last_selection = Some(selection);

    let selected = state.selected_sample;
    for (i, (name, _)) in state.samples.iter().enumerate() {
        let is_sel = i == selected;
        let mut row = commands.spawn((
            Name::new(format!("Sample {i}: {name}")),
            SampleRow(i),
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                margin: UiRect::vertical(Val::Px(1.0)),
                ..default()
            },
            ChildOf(root),
        ));
        if is_sel {
            row.insert(BackgroundColor(Color::srgb(0.85, 0.9, 1.0)));
        }
        let row = row.id();
        commands.spawn((
            Text::new(name.clone()),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(if is_sel {
                Color::srgb(0.1, 0.25, 0.7)
            } else {
                Color::srgb(0.2, 0.2, 0.2)
            }),
            ChildOf(row),
        ));
    }
}

/// Picking observer: a click on a `SampleRow` entity loads that sample.
pub fn on_sample_row_click(
    trigger: On<bevy::picking::events::Pointer<bevy::picking::events::Click>>,
    rows: Query<&SampleRow>,
    mut state: NonSendMut<A2uiState>,
) {
    if let Ok(row) = rows.get(trigger.event().entity) {
        state.load_sample(row.0);
    }
}

/// Drive the sample-browser scroll with the mouse wheel. Bevy 0.18's UI does
/// **not** auto-wire mouse-wheel → `ScrollPosition`, so a node with
/// `Overflow::scroll_y` clips but never moves. This system advances the browser
/// root's `ScrollPosition.y` by the accumulated wheel delta (clamped to ≥ 0 by
/// the layout system). The browser is the only scrollable UI container in the
/// gallery, so we apply any wheel motion to it.
pub fn browser_mouse_wheel(
    scroll_input: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut q: Query<&mut ScrollPosition, With<BrowserRoot>>,
) {
    let delta = scroll_input.delta.y;
    if delta != 0.0 {
        for mut scroll in q.iter_mut() {
            // Wheel-up (positive y) scrolls the list up: subtract from offset.
            // `AccumulatedMouseScroll` is reset to zero each frame by an
            // internal system, so this reads the per-frame scroll delta.
            scroll.0.y -= delta * 32.0;
        }
    }
}
