//! # Example: Capabilities Negotiation + Inline Catalog
//!
//! Demonstrates the A2UI v1.0 capabilities handshake without spinning up a
//! terminal: build the basic catalog, register an inline catalog, and print
//! both the client capabilities JSON and a round-tripped server capabilities
//! payload.
//!
//! ## What it demonstrates
//! - [`MessageProcessor::registered_catalog_ids`] (the IDs the client supports)
//! - [`MessageProcessor::register_inline_catalog`] (schema-only functions)
//! - [`ClientCapabilitiesBuilder`] producing the wire payload
//! - Server capabilities deserialization + re-serialization round trip
//!
//! ## Run
//! ```sh
//! cargo run --example 12_handshake
//! ```

use serde_json::json;

use a2ui::core::capabilities::{
    ClientCapabilitiesBuilder, ClientCapabilitiesEnvelope, ServerCapabilitiesEnvelope,
};
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::basic::build_basic_catalog;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── 1. Build the processor with the native basic catalog ────────────
    let mut processor = MessageProcessor::new(vec![build_basic_catalog()]);

    let native_ids = processor.registered_catalog_ids();
    println!("== Native catalog IDs ==");
    for id in &native_ids {
        println!("  - {id}");
    }

    // ── 2. Register an inline catalog (schema-only functions) ───────────
    let inline_catalog = json!({
        "catalogId": "https://a2ui.org/specification/v1_0/catalogs/inline/catalog.json",
        "components": {
            "Greeting": {}
        },
        "functions": {
            "shout": {
                "returnType": "string",
                "args": {
                    "properties": {"value": {}}
                }
            }
        }
    });
    processor.register_inline_catalog(inline_catalog.clone())?;

    let all_ids = processor.registered_catalog_ids();
    println!("\n== Catalog IDs after inline registration ==");
    for id in &all_ids {
        println!("  - {id}");
    }

    // ── 3. Build the client capabilities payload ────────────────────────
    let client_caps = ClientCapabilitiesBuilder::from_catalog_ids(native_ids.clone())
        .with_inline_catalog(inline_catalog.clone())?
        .build();
    let client_envelope = ClientCapabilitiesEnvelope { v1_0: client_caps };
    let client_json = serde_json::to_string_pretty(&client_envelope)?;

    println!("\n== Client capabilities (a2uiClientCapabilities) ==");
    println!("{client_json}");

    // ── 4. Round-trip a server capabilities payload ─────────────────────
    let server_raw = json!({
        "v1.0": {
            "supportedCatalogIds": [
                "https://a2ui.org/specification/v1_0/catalogs/minimal/catalog.json",
                "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
            ],
            "acceptsInlineCatalogs": true
        }
    });
    let server_env: ServerCapabilitiesEnvelope =
        serde_json::from_value(server_raw).expect("server capabilities should deserialize");
    let server_json = serde_json::to_string_pretty(&server_env)?;

    println!("\n== Server capabilities (a2uiServerCapabilities, round-tripped) ==");
    println!("{server_json}");

    println!(
        "\nHandshake complete: server accepts inline catalogs = {}",
        server_env.v1_0.accepts_inline_catalogs
    );

    Ok(())
}
