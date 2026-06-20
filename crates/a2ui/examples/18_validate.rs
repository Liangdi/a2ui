//! # Example: Validation
//!
//! A2UI payloads often come from untrusted or LLM-generated sources. This
//! example shows the `validate` module — integrity, topology, and payload-fix
//! checks ported from the Python SDK — and the opt-in `MessageProcessor` hook
//! that surfaces them without changing the default load behavior.
//!
//! Unlike the TUI examples, this one is pure stdout — no terminal raw mode.
//!
//! ## What it demonstrates
//! - Direct use of `validate_component_integrity` + `analyze_topology`
//! - STRICT vs RELAXED `ValidationConfig` (incremental updates tolerate dangling refs)
//! - `parse_and_fix` healing malformed JSON (smart quotes, trailing commas)
//! - The opt-in `MessageProcessor::with_validation` hook: components still load,
//!   diagnostics surface via `drain_validation`
//!
//! ## Run
//! ```sh
//! cargo run --example 18_validate
//! ```

use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::validate::{
    RELAXED_VALIDATION, ROOT_ID, RefFieldSpec, STRICT_VALIDATION, ValidationConfig,
    ValidationReport, analyze_topology, parse_and_fix, validate_component_integrity,
};
use serde_json::{Value, json};

fn main() {
    println!("A2UI validate — examples\n");

    // ── 1. A valid component tree passes clean ───────────────────────────
    //    root (Column) -> [title, body]; both children defined.
    let valid = json!([
        { "id": "root",  "component": "Column", "children": ["title", "body"] },
        { "id": "title", "component": "Text",   "text": "Hi" },
        { "id": "body",  "component": "Text",   "text": "World" }
    ]);
    print_section("1. Valid payload (STRICT)");
    run_checks(&valid, STRICT_VALIDATION);

    // ── 2. A messy payload surfaces every problem at once ────────────────
    //    The validator collects ALL findings rather than stopping at the first:
    //      - "dup" is declared twice            -> DuplicateId
    //      - root.child -> "ghost" (undefined)   -> DanglingReference
    //      - "orphan" is unreachable from root   -> OrphanComponent
    //      - root.child -> "root" (self-loop)    -> SelfReference
    let messy = json!([
        { "id": "root",  "component": "Column", "child": "root" },   // self-ref
        { "id": "dup",   "component": "Text",   "text": "first" },
        { "id": "dup",   "component": "Text",   "text": "second" },  // duplicate
        { "id": "orphan","component": "Text",   "text": "lost" }     // unreachable
    ]);
    print_section("2. Messy payload (STRICT) — duplicate / self-ref / orphan");
    run_checks(&messy, STRICT_VALIDATION);

    // ── 3. STRICT vs RELAXED: incremental updates ───────────────────────
    //    A partial `updateComponents` may reference ids that already live on
    //    the client. STRICT flags them as dangling; RELAXED tolerates them.
    let partial = json!([
        { "id": "root", "component": "Column", "children": ["already_here"] }
        // "already_here" is NOT in this batch — it was sent earlier.
    ]);
    print_section("3. Partial update under STRICT (dangling flagged)");
    run_checks(&partial, STRICT_VALIDATION);

    print_section("4. Same partial update under RELAXED (tolerated)");
    run_checks(&partial, RELAXED_VALIDATION);

    // ── 5. parse_and_fix: heal malformed JSON from an LLM ────────────────
    print_section("5. parse_and_fix — smart quotes + trailing comma");
    // Curly double quotes (U+201C/U+201D) and a trailing comma are common
    // LLM artifacts. parse_and_fix normalizes the quotes and strips the comma,
    // turning this back into valid JSON. (It heals quotes/commas only — it
    // won't repair structural problems like a missing object brace, so the
    // payload must already be object-shaped.)
    let malformed = "[{\u{201C}id\u{201D}: \u{201C}root\u{201D}, \u{201C}component\u{201D}: \u{201C}Text\u{201D},}]";
    match parse_and_fix(malformed) {
        Ok(items) => println!("  healed → {} item(s): {:#}", items.len(), items[0]),
        Err(e) => println!("  still broken: {e}"),
    }

    // ── 6. The opt-in MessageProcessor hook ──────────────────────────────
    print_section("6. MessageProcessor opt-in validation");
    // By default validation is OFF — bad payloads load silently (graceful
    // degradation). `.with_validation(STRICT_VALIDATION)` turns on reporting
    // WITHOUT rejecting the load: components still get added, and you read
    // the diagnostics back via `drain_validation`.
    let mut proc = MessageProcessor::new(vec![]).with_validation(STRICT_VALIDATION);

    let create = json!({
        "version": "v1.0",
        "createSurface": { "surfaceId": "demo", "catalogId": "demo" }
    });
    let update = json!({
        "version": "v1.0",
        "updateComponents": {
            "surfaceId": "demo",
            "components": [
                { "id": "root", "component": "Column", "children": ["missing_child"] }
                // "missing_child" never arrives → dangling reference.
            ]
        }
    });
    proc.process_message(MessageProcessor::parse_message(&create.to_string()).unwrap())
        .unwrap();
    proc.process_message(MessageProcessor::parse_message(&update.to_string()).unwrap())
        .unwrap();

    // The component still loaded despite the bad ref...
    let loaded = proc.model.get_surface("demo").is_some();
    println!("  surface loaded despite bad ref? {loaded}");
    // ...and the diagnostic is waiting for us:
    let report = proc.drain_validation();
    print_report(&report, "  ");

    println!("\nDone.");
}

// ── helpers ────────────────────────────────────────────────────────────────

/// Run integrity + topology against a component array and pretty-print the
/// combined report. Demonstrates the two independent checks composing.
fn run_checks(components: &Value, cfg: ValidationConfig) {
    let arr: &[Value] = components.as_array().expect("components is an array");
    let spec = RefFieldSpec::DEFAULT;

    let mut report = validate_component_integrity(
        arr,
        &spec,
        ROOT_ID,
        cfg.allow_dangling_references,
        cfg.allow_missing_root,
    );
    let (_visited, topo) = analyze_topology(
        arr,
        &spec,
        ROOT_ID,
        cfg.allow_orphan_components,
        cfg.allow_missing_root,
    );
    report.extend(topo);

    print_report(&report, "  ");
}

/// Pretty-print a ValidationReport, showing the structured fields of each
/// finding (code, component_id, message). An empty report prints "✓ OK".
fn print_report(report: &ValidationReport, indent: &str) {
    if report.is_empty() {
        println!("{indent}✓ no problems found");
        return;
    }
    for err in &report.errors {
        let where_ = err.component_id.as_deref().unwrap_or("—");
        println!("{indent}• {:?}  [{}]", err.code, where_);
        println!("{indent}  {}", err.message);
    }
}

fn print_section(title: &str) {
    println!("── {title} ──");
}
