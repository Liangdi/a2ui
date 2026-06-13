//! L2 — Full cascade: inheritance, `var()`, specificity, pseudo-states.
//!
//! Builds a tiny component tree (a panel `Column` containing a `Text` and a
//! `Button`) and resolves styles top-down so children inherit from their
//! parent. Also shows `:disabled` flipping a button's appearance.
//!
//! ```sh
//! cargo run -p ratatui-css --example cascade
//! ```

use ratatui::style::Style;

use ratatui_css::{CssStyle, Origin, OwnedNode, State, Stylesheet};

fn show(label: &str, s: Style) {
    println!(
        "  {label:34} fg={:<14?} bg={:<14?} mod={:?}",
        s.fg, s.bg, s.add_modifier
    );
}

fn main() {
    let mut sheet = Stylesheet::new();
    // CSS custom property --accent, resolved via var() during the cascade.
    sheet.tokens_mut().insert("accent", "#00d4ff");

    // Panel passes a text color down to its children via inheritance.
    sheet
        .add("Column.panel", CssStyle::new().color("#cdd6f4").italic(), Origin::Theme)
        .unwrap();
    // A button that uses the accent token.
    sheet
        .add(
            "Button",
            CssStyle::new().background("var(--accent)").bold(),
            Origin::User,
        )
        .unwrap();
    // Disabled buttons are dimmed.
    sheet
        .add("Button:disabled", CssStyle::new().color("gray"), Origin::User)
        .unwrap();

    // --- resolve the tree -------------------------------------------------
    let panel = sheet.compute(&OwnedNode::new("Column").with_classes(["panel"]), None);
    show("Column.panel (root)", panel.to_style());

    // Child Text has no color of its own -> inherits #cdd6f4 + italic.
    let text = sheet.compute(&OwnedNode::new("Text"), Some(&panel));
    show("Text (inherits from panel)", text.to_style());

    // Enabled button: accent background, bold.
    let btn_on = sheet.compute(&OwnedNode::new("Button"), Some(&panel));
    show("Button (enabled)", btn_on.to_style());

    // Disabled button: :disabled rule applies, color=gray.
    let btn_off = sheet.compute(
        &OwnedNode::new("Button").with_state(State::disabled()),
        Some(&panel),
    );
    show("Button (disabled)", btn_off.to_style());

    println!("\nThe Text child inherited color + italic from Column.panel —");
    println!("native ratatui has no inheritance; ratatui-css adds it.");
}
