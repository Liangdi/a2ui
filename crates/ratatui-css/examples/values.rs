//! L0 — Value types only.
//!
//! Build a [`CssStyle`] by hand and from JSON, then project it onto native
//! ratatui `Style` / `Block`. No cascade, no stylesheet — just the building
//! blocks.
//!
//! ```sh
//! cargo run -p ratatui-css --example values
//! ```

use ratatui::style::Style;

use ratatui_css::CssStyle;

fn show(label: &str, s: Style) {
    println!(
        "  {label:28} fg={:<14?} bg={:<14?} modifiers={:?}",
        s.fg, s.bg, s.add_modifier
    );
}

fn main() {
    // --- 1. Builder (Rust-side) -------------------------------------------
    println!("== builder ==");
    let primary = CssStyle::new()
        .color("#ffffff")
        .background("blue")
        .bold()
        .italic()
        .underline();
    show("primary button", primary.to_style());

    let danger = CssStyle::new()
        .color("white")
        .background("#c0392b")
        .bold();
    show("danger button", danger.to_style());

    // --- 2. From JSON (data-driven / server-supplied) ---------------------
    println!("\n== from JSON ==");
    let json = r##"{
        "color": "#00d4ff",
        "background-color": "#1e1e2e",
        "font-weight": "bold",
        "text-decoration": "underline",
        "border": "rounded #444",
        "padding": "1 2"
    }"##;
    let card: CssStyle = serde_json::from_str(json).expect("parse JSON");

    let style = card.to_style();
    show("card decoration", style);

    let block = card.to_block();
    println!("\n  card block (ratatui): {block:?}");

    // Apply padding to a 40x10 area and show the shrunk inner rect.
    let area = ratatui::layout::Rect::new(0, 0, 40, 10);
    let after_padding = block.inner(area);
    println!("  area {area:?} -> inner (padding 1 2) {after_padding:?}");
}
