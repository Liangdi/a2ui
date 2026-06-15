//! A2UI egui gallery — loads an A2UI sample and shows it in an egui window.
//!
//! This is the egui-backend counterpart of the ratatui gallery (`a2ui-gallery`)
//! and the Slint gallery (`a2ui-slint-gallery`): it reuses the same embedded
//! spec samples and the same catalog/function builders, but renders into a real
//! OS window via [`a2ui_egui`].
//!
//! Usage:
//!   a2ui_egui_gallery            # show the first sample
//!   a2ui_egui_gallery 3          # show sample #3 (1-based index)
//!   a2ui_egui_gallery stepper    # show the first sample whose name
//!                                # contains "stepper" (case-insensitive)
//!
//! The list of available samples (index + name) is printed to stdout at startup.

use std::collections::HashMap;
use std::env;
use std::process::ExitCode;

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::protocol::server_to_client::A2uiMessage;
use a2ui_egui::EguiApp;
use a2ui_gallery::sample_loader::{self, Sample};
use a2ui_tui::catalogs::basic::build_basic_catalog;
use a2ui_tui::catalogs::minimal::build_minimal_catalog;

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

    println!("A2UI egui gallery — {} sample(s) available:", samples.len());
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

    // 3. Build the catalogs (basic first, then minimal — order matches the other
    //    galleries).
    let catalogs = vec![build_basic_catalog(), build_minimal_catalog()];

    // 4. Build the function map keyed by function name.
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| (f.name().to_string(), f))
        .collect();

    // 5. Create the egui app and hand it the samples.
    let mut app = EguiApp::new(catalogs, functions);
    let entries: Vec<(String, Vec<A2uiMessage>)> = samples
        .iter()
        .map(|s| (s.name.clone(), s.messages.clone()))
        .collect();
    app.set_samples(entries, selected);

    // 6. Open the window and run the egui event loop until it closes.
    //    `eframe::run_native` takes an `AppCreator` that builds the `eframe::App`
    //    from a `CreationContext`; our `EguiApp` implements `eframe::App`, so we
    //    box it up and hand it over.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_title("A2UI egui Gallery"),
        ..Default::default()
    };
    let run_result = eframe::run_native(
        "A2UI egui Gallery",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    );
    if let Err(err) = run_result {
        eprintln!("egui window error: {err}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
