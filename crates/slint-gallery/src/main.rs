//! A2UI Slint gallery — loads an A2UI sample and shows it in a Slint window.
//!
//! This is the Slint-backend counterpart of the ratatui gallery (`a2ui-gallery`):
//! it reuses the same embedded spec samples and the same catalog/function
//! builders, but renders into a real OS window via [`a2ui_slint`].
//!
//! Usage:
//!   a2ui_slint_gallery            # show the first sample
//!   a2ui_slint_gallery 3          # show sample #3 (1-based index)
//!   a2ui_slint_gallery stepper    # show the first sample whose name
//!                                 # contains "stepper" (case-insensitive)
//!
//! The list of available samples (index + name) is printed to stdout at startup.

use std::collections::HashMap;
use std::env;
use std::process::ExitCode;

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::protocol::server_to_client::A2uiMessage;
use a2ui_gallery::sample_loader::{self, Sample};
use a2ui_slint::host::SurfaceHost;
use a2ui_tui::catalogs::basic::build_basic_catalog;
use a2ui_tui::catalogs::minimal::build_minimal_catalog;

/// Load the samples for a single catalog dir (e.g. `"minimal"`, `"basic"`) from
/// the embedded spec tree, mirroring what the ratatui gallery does. If
/// `A2UI_SPEC_DIR` is set, samples are read from that on-disk directory instead
/// — a dev override for testing spec changes without recompiling.
fn load_catalog_samples(catalog: &str) -> Vec<Sample> {
    let subpath = format!("v1_0/catalogs/{catalog}/examples");
    if let Ok(root) = env::var("A2UI_SPEC_DIR") {
        sample_loader::load_samples_from_dir(&format!("{root}/{subpath}"))
    } else {
        sample_loader::load_samples(&subpath)
    }
}

/// Resolve the user-provided CLI argument to an index into `samples`.
///
/// `arg` may be:
/// - a 1-based index (`"1"`, `"2"`, …) → that index minus one, or `None` if the
///   number is out of range,
/// - any other string → the index of the first sample whose name contains the
///   argument case-insensitively, or `None` if nothing matches.
///
/// Returns `Some(index)` on a match, `None` otherwise.
fn resolve_sample(arg: &str, samples: &[Sample]) -> Option<usize> {
    // First, try to parse as a 1-based index.
    if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= samples.len() {
            return Some(n - 1);
        }
        // A number that doesn't refer to any sample is not a name substring
        // either, so report "no match" rather than silently doing something else.
        return None;
    }

    // Otherwise, treat the argument as a case-insensitive name substring and
    // pick the first matching sample.
    let needle = arg.to_lowercase();
    samples
        .iter()
        .position(|s| s.name.to_lowercase().contains(&needle))
}

fn main() -> ExitCode {
    // ------------------------------------------------------------------
    // 1. Load samples (minimal first, then basic) — same order as the TUI
    //    gallery so indices line up across the two backends.
    // ------------------------------------------------------------------
    let mut samples = load_catalog_samples("minimal");
    samples.extend(load_catalog_samples("basic"));

    if samples.is_empty() {
        eprintln!("No samples found.");
        return ExitCode::from(1);
    }

    // Always print the catalog so the user knows what to pass on the CLI.
    println!(
        "A2UI Slint gallery — {} sample(s) available:",
        samples.len()
    );
    for (i, s) in samples.iter().enumerate() {
        println!("  {:>2}. {} — {}", i + 1, s.name, s.description);
    }

    // ------------------------------------------------------------------
    // 2. Pick which sample to show from the CLI arg (if any).
    // ------------------------------------------------------------------
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

    // ------------------------------------------------------------------
    // 3. Build the catalogs. Both are registered so samples from either dir
    //    resolve their component/component-definition lookups. Order matches
    //    the TUI gallery (basic first, then minimal).
    // ------------------------------------------------------------------
    let catalogs = vec![build_basic_catalog(), build_minimal_catalog()];

    // ------------------------------------------------------------------
    // 4. Build the function map keyed by function name.
    // ------------------------------------------------------------------
    let functions: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
        .into_iter()
        .map(|f| {
            let name = f.name().to_string();
            (name, f)
        })
        .collect();

    // ------------------------------------------------------------------
    // 5. Create the Slint surface host (opens no window yet — `run()` does).
    // ------------------------------------------------------------------
    let host = match SurfaceHost::new(catalogs, functions) {
        Ok(host) => host,
        Err(err) => {
            eprintln!("Failed to create Slint surface host: {err}");
            return ExitCode::from(1);
        }
    };

    // ------------------------------------------------------------------
    // 6. Hand all samples to the host. It builds the left-hand sample browser
    //    and loads the CLI-selected sample (or the first) into the right pane;
    //    clicking a row in the sidebar switches samples live.
    // ------------------------------------------------------------------
    let entries: Vec<(String, Vec<A2uiMessage>)> = samples
        .iter()
        .map(|s| (s.name.clone(), s.messages.clone()))
        .collect();
    host.set_samples(entries, selected);

    // ------------------------------------------------------------------
    // 7. Show the window and block until it's closed.
    // ------------------------------------------------------------------
    if let Err(err) = host.run() {
        eprintln!("Slint window error: {err}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
