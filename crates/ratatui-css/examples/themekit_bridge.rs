//! P2 — themekit interop. (Requires the `themekit` feature.)
//!
//! Seeds CSS custom properties from a `ratatui-themekit` theme's 15 semantic
//! slots, then writes rules in terms of `var(--accent)` / `var(--text)` / …
//! and lets the cascade resolve them through the existing palette.
//!
//! ```sh
//! cargo run -p ratatui-css --example themekit_bridge --features themekit
//! ```

use ratatui::style::Style;
use ratatui_themekit::CatppuccinMocha;

use ratatui_css::{CssStyle, Origin, OwnedNode, Stylesheet, ThemeTokens};

fn show(label: &str, s: Style) {
    println!("  {label:14} fg={:<14?} bg={:<14?} mod={:?}", s.fg, s.bg, s.add_modifier);
}

fn main() {
    // Bridge: themekit theme -> CSS custom properties.
    let tokens = ThemeTokens::from_themekit(&CatppuccinMocha);
    let mut sheet = Stylesheet::with_tokens(tokens);

    sheet.add("Button", CssStyle::new().color("var(--text)").background("var(--accent)").bold(), Origin::User).unwrap();
    sheet.add("Text", CssStyle::new().color("var(--text)"), Origin::User).unwrap();
    sheet.add("Text.error", CssStyle::new().color("var(--error)").bold(), Origin::User).unwrap();
    sheet.add("Text.muted", CssStyle::new().color("var(--text-dim)"), Origin::User).unwrap();

    let cases: &[(&str, OwnedNode)] = &[
        ("Button", OwnedNode::new("Button")),
        ("Text", OwnedNode::new("Text")),
        ("Text.error", OwnedNode::new("Text").with_classes(["error"])),
        ("Text.muted", OwnedNode::new("Text").with_classes(["muted"])),
    ];

    println!("Catppuccin Mocha driving the cascade:");
    for (label, node) in cases {
        let computed = sheet.compute(node, None);
        show(label, computed.to_style());
    }
}
