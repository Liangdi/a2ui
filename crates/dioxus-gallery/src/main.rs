//! A2UI Dioxus gallery — loads an A2UI sample and shows it in a Dioxus WebView
//! window.
//!
//! This is the Dioxus-backend counterpart of the ratatui gallery (`a2ui-gallery`),
//! the Slint gallery (`a2ui-slint-gallery`), the egui gallery
//! (`a2ui-egui-gallery`), the bevy gallery (`a2ui-bevy-gallery`), and the Iced
//! gallery (`a2ui-iced-gallery`): it reuses the same embedded spec samples and
//! the same catalog/function builders, but renders into a real OS WebView via
//! [`a2ui_dioxus`] (Dioxus's reactive-signals architecture).
//!
//! Usage:
//!   a2ui_dioxus_gallery            # show the first sample
//!   a2ui_dioxus_gallery 3          # show sample #3 (1-based index)
//!   a2ui_dioxus_gallery stepper     # show the first sample whose name
//!                                # contains "stepper" (case-insensitive)
//!
//! The list of available samples (index + name) is printed to stdout at startup.

use std::collections::{HashMap, HashSet};
use std::env;
use std::process::ExitCode;
use std::rc::Rc;

use a2ui_base::catalog::basic_functions::build_basic_functions;
use a2ui_base::catalog::function_api::FunctionImplementation;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::protocol::server_to_client::A2uiMessage;
use a2ui_dioxus::{Gallery, STYLESHEET};
use a2ui_gallery::sample_loader::{self, Sample};
use a2ui_tui::catalogs::basic::build_basic_catalog;
use a2ui_tui::catalogs::minimal::build_minimal_catalog;

use dioxus::desktop::tao::dpi::LogicalSize;
use dioxus::desktop::{Config, WindowBuilder};
use dioxus::prelude::*;

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

    println!("A2UI dioxus gallery — {} sample(s) available:", samples.len());
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

    // 3. Pack the entries + initial index for the Dioxus app. These two are the
    //    only values that must cross the `launch(app: fn() -> Element)` boundary,
    //    so they are injected into the root context via `with_context` (which
    //    requires `Clone + Send + Sync + 'static` — both trivially satisfy it).
    //    The catalogs + function map are rebuilt inside `app()` (below), since
    //    they are non-`Clone` and so can't ride through `with_context`.
    let entries: Vec<(String, Vec<A2uiMessage>)> = samples
        .iter()
        .map(|s| (s.name.clone(), s.messages.clone()))
        .collect();
    let cfg = GalleryConfig { entries, selected };

    // 4. Launch the Dioxus desktop app. The window carries the gallery title, a
    //    1080×740 initial size, and the dark Catppuccin-green stylesheet
    //    injected into the document head.
    dioxus::LaunchBuilder::new()
        .with_context(cfg)
        .with_cfg(desktop! {
            Config::new()
                .with_window(
                    WindowBuilder::new()
                        .with_title("A2UI · Dioxus Gallery")
                        .with_inner_size(LogicalSize::new(1080.0, 740.0)),
                )
                .with_custom_head(format!("<style>{STYLESHEET}</style>"))
        })
        .launch(app);

    // `launch` blocks on the WebView event loop (never returns in practice),
    // but it is typed `-> ()`, so satisfy the `ExitCode` return explicitly.
    ExitCode::SUCCESS
}

/// The boot data injected into the root context by `LaunchBuilder::with_context`.
/// Must be `Clone + Send + Sync + 'static` — the launch-time injection contract.
#[derive(Clone)]
struct GalleryConfig {
    entries: Vec<(String, Vec<A2uiMessage>)>,
    selected: usize,
}

/// The Dioxus root — reads the boot data out of context, then builds the
/// non-`Clone` runtime state (the `MessageProcessor` + function map) into
/// signals and shares them via context so the prop-less [`Gallery`] chrome and
/// the recursive node renderer can read them.
///
/// This is the Dioxus analog of the Iced gallery's boot closure that builds an
/// `IcedApp::new(catalogs, functions)` — but, because Dioxus component props
/// must be `Clone + PartialEq` (and the processor/function map aren't), the
/// state is built here and threaded through context rather than props.
fn app() -> Element {
    let cfg = use_context::<GalleryConfig>();

    // Build the merged function map once (read-only), shared via `Rc`.
    let functions: Rc<HashMap<String, Box<dyn FunctionImplementation>>> =
        use_hook(|| Rc::new(build_function_map()));

    // Build + replay the initial sample into the processor signal. The signal
    // initializer is `FnOnce`, so it may consume the freshly-built catalogs.
    let processor: Signal<MessageProcessor> = use_signal(|| {
        let catalogs = vec![build_basic_catalog(), build_minimal_catalog()];
        let mut p = MessageProcessor::new(catalogs);
        if let Some(msgs) = cfg.entries.get(cfg.selected).map(|(_, m)| m.clone()) {
            for msg in &msgs {
                let _ = p.process_message(msg.clone());
            }
        }
        p
    });
    let selected: Signal<usize> = use_signal(|| cfg.selected);
    let open_modals: Signal<HashSet<String>> = use_signal(HashSet::new);
    let focused: Signal<Option<String>> = use_signal(|| None);
    let samples: Rc<Vec<(String, Vec<A2uiMessage>)>> = use_hook(|| Rc::new(cfg.entries.clone()));

    // Share everything with the Gallery chrome + the recursive node renderer.
    use_context_provider(|| processor);
    use_context_provider(|| functions);
    use_context_provider(|| selected);
    use_context_provider(|| open_modals);
    use_context_provider(|| focused);
    use_context_provider(|| samples);

    rsx! { Gallery {} }
}

/// Build the function map keyed by function name (mirrors the other galleries).
fn build_function_map() -> HashMap<String, Box<dyn FunctionImplementation>> {
    build_basic_functions()
        .into_iter()
        .map(|f| (f.name().to_string(), f))
        .collect()
}
