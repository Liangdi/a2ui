//! A2UI GPUI gallery — loads an A2UI sample and shows it in a GPUI window.
//!
//! This is the GPUI-backend counterpart of the ratatui gallery (`a2ui-gallery`),
//! the Slint gallery (`a2ui-slint-gallery`), the egui gallery
//! (`a2ui-egui-gallery`), the bevy gallery (`a2ui-bevy-gallery`), and the iced
//! gallery (`a2ui-iced-gallery`): it reuses the same embedded spec samples and
//! the same catalog/function builders, but renders into a real OS window via
//! [`a2ui_gpui`] (Zed's GPUI, through the gpui-component widget set).
//!
//! Usage:
//!   a2ui_gpui_gallery            # show the first sample
//!   a2ui_gpui_gallery 3          # show sample #3 (1-based index)
//!   a2ui_gpui_gallery stepper     # show the first sample whose name
//!                                # contains "stepper" (case-insensitive)
//!
//! The list of available samples (index + name) is printed to stdout at startup.

use std::collections::HashMap;
use std::env;
use std::process::ExitCode;

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::protocol::server_to_client::A2uiMessage;
use a2ui_gallery::sample_loader::{self, Sample};
use a2ui_gpui::{GpuiApp, customize};
use a2ui_tui::catalogs::basic::build_basic_catalog;
use a2ui_tui::catalogs::minimal::build_minimal_catalog;
use gpui::prelude::*;
use gpui::{
    Application, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use gpui_component::{Root, init as init_component};
use gpui_component_assets::Assets;

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

    println!("A2UI gpui gallery — {} sample(s) available:", samples.len());
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

    // 5. Create the GPUI app and hand it the samples.
    let entries: Vec<(String, Vec<A2uiMessage>)> = samples
        .iter()
        .map(|s| (s.name.clone(), s.messages.clone()))
        .collect();

    // 6. Run the GPUI loop. `Application::with_assets(Assets)` bundles the
    //    fonts + icon set `gpui_component::init` needs; `init_component`
    //    registers the theme / global state / focus bindings; `customize`
    //    flips the theme to dark + the gallery's green accent.
    //
    //    The window's root view MUST be a `gpui_component::Root` (it owns the
    //    dialog / sheet / notification layers + focus forwarding). Our
    //    `GpuiApp` is built inside `cx.new` (a one-time `FnOnce`), wrapped in
    //    `Root::new(view.into(), window, cx)`.
    Application::new()
        .with_assets(Assets)
        .run(move |cx| {
            init_component(cx);
            customize(cx);

            let window_options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    size(px(1080.), px(740.)),
                    cx,
                )),
                titlebar: Some(TitlebarOptions {
                    title: Some("A2UI · GPUI Gallery".into()),
                    // `appears_transparent` hides the OS titlebar so the
                    // gpui-component `TitleBar` (rendered in `GpuiApp::render`)
                    // can draw its own — the cross-platform CSD pattern.
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                // GNOME Wayland ignores server-side decoration requests (its
                // policy is CSD-only), so the default `Server` leaves the
                // window with no titlebar at all — undraggable, no close
                // button. `Client` makes gpui draw its own decorations (the
                // `TitleBar` provides drag + min/max/close), which GNOME
                // accepts. (Harmless on X11 / macOS.)
                window_decorations: Some(gpui::WindowDecorations::Client),
                ..Default::default()
            };

            cx.spawn(async move |cx| {
                cx.open_window(window_options, |window, cx| {
                    let view = cx.new(|_cx| {
                        let mut app = GpuiApp::new(catalogs, functions);
                        app.set_samples(entries, selected);
                        app
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                })?;
                Ok::<_, anyhow::Error>(())
            })
            .detach();
        });

    ExitCode::SUCCESS
}
