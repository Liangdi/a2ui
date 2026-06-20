//! A2UI Bevy gallery — loads an A2UI sample and shows it in a Bevy window.
//!
//! This is the Bevy-backend counterpart of the ratatui gallery (`a2ui-gallery`),
//! the Slint gallery (`a2ui-slint-gallery`), and the egui gallery
//! (`a2ui-egui-gallery`): it reuses the same embedded spec samples and the same
//! catalog/function builders, but renders into a real OS window via Bevy's ECS
//! UI stack.
//!
//! Usage:
//!   a2ui_bevy_gallery            # show the first sample
//!   a2ui_bevy_gallery 3          # show sample #3 (1-based index)
//!   a2ui_bevy_gallery stepper    # show the first sample whose name
//!                                # contains "stepper" (case-insensitive)
//!
//! The list of available samples (index + name) is printed to stdout at startup.

use std::collections::HashMap;
use std::env;
use std::process::ExitCode;

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_bevy::{A2uiPlugin, A2uiState};
use a2ui_gallery::sample_loader::{self, Sample};
use a2ui_tui::catalogs::basic::build_basic_catalog;
use a2ui_tui::catalogs::minimal::build_minimal_catalog;
use bevy::prelude::*;

/// Load the samples for a single catalog dir (e.g. `"minimal"`, `"basic"`) from
/// the embedded spec tree, mirroring what the other galleries do. If
/// `A2UI_SPEC_DIR` is set, samples are read from that on-disk directory instead.
fn load_catalog_samples(catalog: &str) -> Vec<Sample> {
    let subpath = format!("v1_0/catalogs/{catalog}/examples");
    if let Ok(root) = env::var("A2UI_SPEC_DIR") {
        sample_loader::load_samples_from_dir(&format!("{root}/{subpath}"))
    } else {
        sample_loader::load_samples(&subpath)
    }
}

/// Resolve the user-provided CLI argument to an index into `samples`:
/// a 1-based index, else the first sample whose name contains the arg
/// (case-insensitive). `None` if nothing matches.
fn resolve_sample(arg: &str, samples: &[Sample]) -> Option<usize> {
    if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= samples.len() {
            return Some(n - 1);
        }
        return None;
    }
    let needle = arg.to_lowercase();
    samples
        .iter()
        .position(|s| s.name.to_lowercase().contains(&needle))
}

fn main() -> ExitCode {
    // 1. Load samples (minimal first, then basic) — same order as the other
    //    galleries so indices line up across backends.
    let mut samples = load_catalog_samples("minimal");
    samples.extend(load_catalog_samples("basic"));

    if samples.is_empty() {
        eprintln!("No samples found.");
        return ExitCode::from(1);
    }

    println!("A2UI Bevy gallery — {} sample(s) available:", samples.len());
    for (i, s) in samples.iter().enumerate() {
        println!("  {:>2}. {} — {}", i + 1, s.name, s.description);
    }

    // 2. Pick which sample to show from the CLI arg (if any).
    let args: Vec<String> = env::args().skip(1).collect();
    let selected = match args.first().map(|s| s.as_str()) {
        None => 0,
        Some(arg) => match resolve_sample(arg, &samples) {
            Some(idx) => idx,
            None => {
                eprintln!(
                    "No sample matches `{}`. Pass a 1-based index or a name \
                     substring (see the list printed above).",
                    arg
                );
                return ExitCode::from(1);
            }
        },
    };

    let sample = &samples[selected];
    println!(
        "\nShowing sample #{}: {} ({} messages)",
        selected + 1,
        sample.name,
        sample.messages.len()
    );

    // 3. Build the catalogs (basic first, then minimal — order matches the
    //    other galleries).
    let catalogs = vec![build_basic_catalog(), build_minimal_catalog()];

    // 4. Build the function map keyed by function name.
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().to_string(), f))
        .collect();

    // 5. Construct the A2UI runtime state and load the samples.
    let mut state = A2uiState::new(catalogs, functions);
    let entries: Vec<(
        String,
        Vec<a2ui_base::protocol::server_to_client::A2uiMessage>,
    )> = samples
        .iter()
        .map(|s| (s.name.clone(), s.messages.clone()))
        .collect();
    state.set_samples(entries, selected);

    // 6. Run the Bevy app. `DefaultPlugins` carries windowing (winit), the UI
    //    plugin, picking, input focus, and the wgpu render backend; `A2uiPlugin`
    //    adds the widget runtimes + the render-loop systems. `A2uiState` is a
    //    NonSend resource (the processor is !Sync).
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "A2UI Bevy Gallery".into(),
            resolution: bevy::window::WindowResolution::new(1000, 700),
            ..default()
        }),
        ..default()
    }))
    .insert_non_send_resource(state)
    .add_plugins(A2uiPlugin);

    // Optional self-screenshot mode (compositor-independent, like the sci-fi HUD
    // example): warm up a few dozen frames, capture one PNG to the path, exit.
    // `A2UI_OPEN_MODALS=1` also force-opens every Modal's overlay first (so a
    // screenshot can show the modal chrome), and is useful for non-screenshot
    // debugging too.
    let screenshot = std::env::var("A2UI_SCREENSHOT_PATH").ok();
    let open_modals = std::env::var("A2UI_OPEN_MODALS")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if open_modals || screenshot.is_some() {
        app.insert_resource(ForceOpenModals(open_modals));
        // Run before the reconciler so the overlay content mounts same-frame.
        app.add_systems(bevy::prelude::PreUpdate, force_open_modals);
    }
    if let Some(path) = screenshot.clone() {
        app.insert_resource(CaptureRequest {
            path: std::path::PathBuf::from(path),
            frame: 0,
        })
        .add_systems(Update, capture_and_exit);
    }

    app.run();

    ExitCode::SUCCESS
}

// ── Optional debug: force-open modals + self-screenshot ──────────────────────

#[derive(Resource, Default)]
struct ForceOpenModals(bool);

/// Insert every `Modal` component id into `open_modals` so the reconciler mounts
/// the overlay content (for screenshots / debugging modal chrome). Driven by
/// `A2UI_OPEN_MODALS=1`.
fn force_open_modals(force: Res<ForceOpenModals>, mut state: NonSendMut<A2uiState>) {
    if !force.0 {
        return;
    }
    // Collect Modal ids while borrowing the model, then mutate open_modals after.
    let modal_ids: Vec<String> = {
        let Some(surface) = state.processor.model.surfaces().next() else {
            return;
        };
        let components = surface.components.borrow();
        components
            .all()
            .iter()
            .filter_map(|(id, m)| (m.component_type == "Modal").then(|| id.clone()))
            .collect()
    };
    let mut changed = false;
    for id in modal_ids {
        if state.open_modals.insert(id) {
            changed = true;
        }
    }
    if changed {
        state.dirty = true;
    }
}

#[derive(Resource)]
struct CaptureRequest {
    path: std::path::PathBuf,
    frame: u32,
}

/// Warm the render up for ~0.75 s, capture one PNG to `path`, then quit once the
/// async save has flushed. Mirrors the sci-fi HUD example's capture path.
fn capture_and_exit(
    mut req: ResMut<CaptureRequest>,
    mut commands: Commands,
    mut exit: bevy::prelude::MessageWriter<bevy::prelude::AppExit>,
) {
    req.frame += 1;
    if req.frame == 45 {
        eprintln!(
            "Capturing Bevy gallery screenshot -> {}",
            req.path.display()
        );
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(
                req.path.clone(),
            ));
    }
    if req.frame == 100 {
        exit.write(bevy::app::AppExit::Success);
    }
}
