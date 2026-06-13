//! Real ratatui integration — render a computed style into an offscreen
//! `Buffer` and dump it as text. No TTY needed: fully deterministic.
//!
//! ```sh
//! cargo run -p ratatui-css --example render
//! ```

use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use ratatui_css::{CssStyle, Origin, OwnedNode, Stylesheet};

fn main() {
    let mut sheet = Stylesheet::new();
    sheet
        .add(
            "Card",
            CssStyle::new()
                .border("rounded #00d4ff")
                .padding("1")
                .background("#1e1e2e"),
            Origin::User,
        )
        .unwrap();

    let computed = sheet.compute(&OwnedNode::new("Card"), None);
    let block = computed.to_block().title(" ratatui-css ");

    let area = Rect::new(0, 0, 44, 8);
    let mut buf = Buffer::empty(area);
    block.render(area, &mut buf);

    println!("Rendered `Card` block into a Buffer ({area:?}):\n");
    dump(&buf, area);
    println!("\nThe block style: {:?}", computed.to_style());
}

fn dump(buf: &Buffer, area: Rect) {
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        println!("│{line}│");
    }
}
