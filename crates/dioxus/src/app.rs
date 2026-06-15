//! `Gallery` — the Dioxus root chrome component, the counterpart of the Iced
//! [`IcedApp`](../../iced/src/app.rs) and the egui [`EguiApp`].
//!
//! Dioxus component props must be `Clone + PartialEq`, so the non-`Clone`
//! runtime state (the `MessageProcessor`, the function map) cannot arrive as a
//! prop. Instead the gallery host's `app()` builds those into [`Signal`]s and
//! shares them via context; this component reads every piece of state out of
//! context and renders the branded sidebar + preview pane + modal overlay. (The
//! signal setup lives in the host because it is the one with access to the
//! catalog/function builders — mirroring how the Iced gallery builds the
//! `IcedApp` and hands it catalogs + functions.)
//!
//! Within the chrome, Dioxus is *reactive-signals*: the UI is a pure read of the
//! context signals. So — unlike the Iced backend (Elm `view`/`update`, needs a
//! `Message` enum) or the egui backend (immediate mode, needs an `EditBuffers`
//! bridge) — interactive widgets in the node tree read straight from the
//! `processor` signal and write straight back through it. **No message enum, no
//! state bridge.** The signal *is* the interaction channel.
//!
//! The one piece of local logic the chrome owns is the activation flow
//! (`handle_activate`) — a Button press → `dispatch_event` + `apply_event_result`
//! + local Modal toggle — ported verbatim from `IcedApp::handle_activate` /
//! `apply_modal_interaction`; only the state access changes from `&mut self` to
//! signal writes.

use std::collections::HashSet;
use std::rc::Rc;

use a2ui_base::components::dispatch_event;
use a2ui_base::event::{InputEvent, InputKey};
use a2ui_base::interaction::apply_event_result;
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::server_to_client::A2uiMessage;

use dioxus::prelude::*;

use crate::node::{A2uiNode, Functions, OnActivate};

/// The Dioxus gallery chrome — a prop-less component that reads all runtime
/// state from context and renders the sidebar + preview pane + modal overlay.
///
/// The host's `app()` must first provide these context values:
/// - `Signal<MessageProcessor>` — the framework-agnostic runtime.
/// - `Signal<usize>` — the active sidebar sample index.
/// - `Signal<HashSet<String>>` — the locally-tracked open Modals.
/// - `Signal<Option<String>>` — the focused component id (kept for parity).
/// - `Rc<Functions>` — the merged function map.
/// - `Rc<Vec<(String, Vec<A2uiMessage>)>>` — the `(name, messages)` sample list.
///
/// This component then builds + provides the activation callback (`OnActivate`)
/// for the recursive node renderer.
#[component]
pub fn Gallery() -> Element {
    let processor: Signal<MessageProcessor> = use_context();
    let functions: Rc<Functions> = use_context();
    let selected: Signal<usize> = use_context();
    let open_modals: Signal<HashSet<String>> = use_context();
    let samples: Rc<Vec<(String, Vec<A2uiMessage>)>> = use_context();

    // Build the activation callback once; the signals it captures are `Copy`.
    let on_activate =
        use_hook(|| make_on_activate(processor, functions.clone(), open_modals));
    use_context_provider(|| on_activate.clone());

    // Deterministic overlay order: iterate modals sorted by id.
    let mut modal_ids: Vec<String> = open_modals.read().iter().cloned().collect();
    modal_ids.sort();

    rsx! {
        div { class: "app",
            Sidebar { samples: samples.clone(), selected, processor, open_modals }
            MainPane { samples: samples.clone(), selected }
        }
        for modal_id in modal_ids {
            ModalOverlay { modal_id }
        }
    }
}

/// Left-hand sample browser — branded header, scrollable list of selectable
/// sample rows, count footer. Clicking a row resets the processor + replays.
#[component]
fn Sidebar(
    samples: Rc<Vec<(String, Vec<A2uiMessage>)>>,
    selected: Signal<usize>,
    processor: Signal<MessageProcessor>,
    open_modals: Signal<HashSet<String>>,
) -> Element {
    let count = samples.len();
    let sel = *selected.read();
    // Pre-clone an Rc per row so each row's onclick closure owns its handle
    // (the rsx `for` body can't start with a `let`, and a single shared `Rc`
    // can't be moved into more than one closure).
    let rows: Vec<(usize, String, Rc<Vec<(String, Vec<A2uiMessage>)>>)> = samples
        .iter()
        .enumerate()
        .map(|(i, (name, _))| (i, name.clone(), samples.clone()))
        .collect();
    rsx! {
        aside { class: "sidebar",
            div { class: "sidebar__brand",
                span { class: "sidebar__mark", "◆" }
                span { class: "sidebar__title",
                    b { "A2UI" }
                    span { "Dioxus Gallery" }
                }
            }
            hr {}
            div { class: "sidebar__section mono", "SAMPLES" }
            div { class: "sidebar__list",
                for (i, name, row_samples) in rows {
                    button {
                        key: "{i}",
                        class: if i == sel { "sample sample--sel" } else { "sample" },
                        onclick: move |_| {
                            load_sample(processor, selected, open_modals, &row_samples, i);
                        },
                        span { class: "sample__idx mono", "{i + 1}" }
                        span { class: "sample__name", "{name}" }
                    }
                }
            }
            hr {}
            div { class: "sidebar__foot mono", "{count} samples" }
        }
    }
}

/// The main pane — breadcrumb top bar (Preview / <sample> · index chip) over
/// the rendered preview surface.
#[component]
fn MainPane(samples: Rc<Vec<(String, Vec<A2uiMessage>)>>, selected: Signal<usize>) -> Element {
    let sel = *selected.read();
    let name = samples.get(sel).map(|(n, _)| n.clone()).unwrap_or_default();
    let count = samples.len();
    rsx! {
        main { class: "main",
            div { class: "topbar",
                span { class: "topbar__crumb mono", "Preview" }
                span { class: "topbar__sep", "›" }
                span { class: "topbar__title", "{name}" }
                span { class: "spacer" }
                span { class: "topbar__chip mono", "{sel + 1} / {count}" }
            }
            hr {}
            div { class: "preview",
                A2uiNode { id: "root".to_string(), base_path: "".to_string() }
            }
        }
    }
}

/// One open Modal's `content` subtree in a centered elevated panel with a title
/// bar + close button, layered over a dimmed click-to-dismiss scrim.
#[component]
fn ModalOverlay(modal_id: String) -> Element {
    let processor: Signal<MessageProcessor> = use_context();

    // Resolve the content id + title in a short borrow.
    let (content_id, title) = {
        let p = processor.read();
        let Some(surface) = p.model.surfaces().next() else {
            return rsx! { span {} };
        };
        let components = surface.components.borrow();
        let Some(m) = components.get(&modal_id) else {
            return rsx! { span {} };
        };
        if m.component_type != "Modal" {
            return rsx! { span {} };
        }
        let content = m.get_property::<String>("content");
        let title = m
            .get_property::<String>("title")
            .unwrap_or_else(|| "Dialog".to_string());
        (content, title)
    };
    let Some(content_id) = content_id else {
        return rsx! { span {} };
    };
    let mut open_modals: Signal<HashSet<String>> = use_context();
    // Each dismiss closure owns its own clone — a `String` isn't `Copy`, so two
    // `move` closures can't share one `modal_id`.
    let scrim_id = modal_id.clone();
    let close_id = modal_id.clone();

    rsx! {
        div { class: "modal-wrap",
            button {
                class: "scrim",
                onclick: move |_| {
                    open_modals.write().remove(&scrim_id);
                },
            }
            div { class: "modal",
                div { class: "modal__head",
                    span { class: "modal__title", "{title}" }
                    span { class: "spacer" }
                    button {
                        class: "btn btn--borderless",
                        onclick: move |_| {
                            open_modals.write().remove(&close_id);
                        },
                        "✕"
                    }
                }
                hr {}
                div { class: "col",
                    A2uiNode { id: content_id, base_path: "".to_string() }
                }
            }
        }
    }
}

/// Load sample `idx`: reset the processor (keeping catalogs), replay its
/// messages, clear modals, set the selection. Signals are passed in — this is a
/// plain fn (not a component), so it cannot itself call `use_context`.
fn load_sample(
    mut processor: Signal<MessageProcessor>,
    mut selected: Signal<usize>,
    mut open_modals: Signal<HashSet<String>>,
    samples: &Rc<Vec<(String, Vec<A2uiMessage>)>>,
    idx: usize,
) {
    let msgs = samples.get(idx).map(|(_, m)| m.clone()).unwrap_or_default();
    {
        let mut p = processor.write();
        p.reset();
        for msg in &msgs {
            let _ = p.process_message(msg.clone());
        }
    }
    open_modals.write().clear();
    selected.set(idx);
}

/// Build the activation callback the recursive node renderer hands a Button
/// press up through. Captures the signals it needs (all `Copy`), so the
/// returned `Rc<dyn Fn(String)>` is `Fn` + `'static`.
///
/// Ported from `IcedApp::handle_activate` + `apply_modal_interaction`: dispatch
/// `Enter` via the shared core logic, apply the result, then resolve any local
/// Modal state change.
fn make_on_activate(
    processor: Signal<MessageProcessor>,
    functions: Rc<Functions>,
    open_modals: Signal<HashSet<String>>,
) -> OnActivate {
    Rc::new(move |node_id: String| {
        // `Fn` closures (this `Rc<dyn Fn>`) can't mutate captures, but `Signal`
        // is `Copy`: copying the handle into a fresh `mut` local and writing
        // through it writes to the same underlying data — so the closure stays
        // `Fn` while still mutating the shared processor.
        let mut proc = processor;
        // ── dispatch Enter + apply the result ───────────────────────────────
        {
            let result = {
                let p = proc.read();
                let Some(surface) = p.model.surfaces().next() else {
                    return;
                };
                let Some(comp_type) = surface
                    .components
                    .borrow()
                    .get(&node_id)
                    .map(|m| m.component_type.clone())
                else {
                    return;
                };
                let data_model = surface.data_model.borrow();
                let components = surface.components.borrow();
                let ctx = ComponentContext::new(
                    node_id.clone(),
                    surface.id.clone(),
                    &data_model,
                    &components,
                    functions.as_ref(),
                    "",
                    Some(node_id.clone()),
                );
                dispatch_event(
                    &comp_type,
                    &ctx,
                    &InputEvent::KeyPress { key: InputKey::Enter },
                )
            };
            if let Some(result) = result {
                let mut p = proc.write();
                let _ = apply_event_result(&mut p, result);
            }
        }
        // ── resolve local Modal state change ────────────────────────────────
        apply_modal_interaction(proc, open_modals, &node_id);
    })
}

/// Resolve a node activation into a local Modal state change. Activating a
/// component that is some Modal's `trigger` opens that Modal; activating a
/// Modal node directly toggles it closed. Ported from the Iced/egui hosts.
fn apply_modal_interaction(
    processor: Signal<MessageProcessor>,
    mut open_modals: Signal<HashSet<String>>,
    node_id: &str,
) {
    let modal_id = {
        let p = processor.read();
        let Some(surface) = p.model.surfaces().next() else {
            return;
        };
        let components = surface.components.borrow();
        let is_modal = components
            .get(node_id)
            .map(|m| m.component_type == "Modal")
            .unwrap_or(false);
        if is_modal {
            // Toggle this Modal: insert returns true if it was newly added.
            let mut mods = open_modals.write();
            if mods.insert(node_id.to_string()) {
                return; // was closed → now open
            }
            Some(node_id.to_string()) // was open → close
        } else {
            // Opening a Modal whose trigger is this node.
            components
                .all()
                .iter()
                .find_map(|(id, m)| {
                    (m.component_type == "Modal"
                        && m.get_property::<String>("trigger").as_deref() == Some(node_id))
                        .then(|| id.clone())
                })
        }
    };

    match modal_id {
        Some(id) if id == node_id => {
            open_modals.write().remove(&id);
        }
        Some(id) => {
            open_modals.write().insert(id);
        }
        None => {}
    }
}
