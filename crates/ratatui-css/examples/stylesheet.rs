//! L1 — Stylesheet + class/id resolution.
//!
//! Parse a CSS text stylesheet (with `:root` tokens, type/class/id selectors,
//! and a `:focus` pseudo-class) and resolve the computed style for several
//! sample nodes. Demonstrates specificity and `var()` resolution.
//!
//! ```sh
//! cargo run -p ratatui-css --example stylesheet
//! ```

use ratatui::style::Style;

use ratatui_css::{OwnedNode, State, Stylesheet};

fn show(label: &str, s: Style) {
    println!(
        "  {label:22} fg={:<14?} bg={:<14?} mod={:?}",
        s.fg, s.bg, s.add_modifier
    );
}

fn main() {
    let css = r##"
        :root {
            --accent: #00d4ff;
        }

        /* type rule: lowest specificity */
        Button   { color: gray; }

        /* class rule: higher specificity */
        Button.primary {
            background: blue;
            color: var(--accent);
        }

        /* id rule: highest specificity */
        #save { color: yellow; }

        /* pseudo-state */
        Button:focus { background: green; }
    "##;

    let sheet = Stylesheet::parse(css).expect("parse stylesheet");

    let cases: &[(&str, OwnedNode)] = &[
        ("Button", OwnedNode::new("Button")),
        ("Button.primary", OwnedNode::new("Button").with_classes(["primary"])),
        (
            "Button.primary + :focus",
            OwnedNode::new("Button").with_classes(["primary"]).with_state(State::focus()),
        ),
        ("#save", OwnedNode::new("Button").with_id("save")),
        ("Text (unstyled)", OwnedNode::new("Text")),
    ];

    println!("Resolved styles:");
    for (label, node) in cases {
        let computed = sheet.compute(node, None);
        show(label, computed.to_style());
    }

    println!("\nNote: Button.primary resolves color:var(--accent) -> Cyan, but");
    println!("#save wins on color via id specificity, and :focus flips the bg.");
}
