# ratatui-css

A CSS cascade engine for [ratatui](https://ratatui.rs) — selectors, specificity,
inheritance, pseudo-states, and data-driven styling. It produces **native
ratatui `Style` / `Block` / `Constraint` values**; it is never a parallel
rendering stack.

It speaks CSS property names (`color`, `background-color`, `font-weight`,
`border`, `padding`, `margin`, `text-align`, `width`, …), is `serde`-friendly
(so server-driven UIs can ship style over the wire), and implements a pragmatic
cascade: **origin × specificity × inheritance × pseudo-states**.

See [`design.md`](design.md) for the full RFC.

## Position in the ecosystem

| Crate | Role | `ratatui-css` |
|---|---|---|
| `ratatui-themekit` | 15 semantic color slots + palettes | **composes** — `ThemeTokens::from_themekit` seeds CSS variables |
| `tui-theme-builder` | compile-time `Style` macro | `ratatui-css` covers the **runtime/config-driven** case |
| `lipgloss` | "CSS for terminals" (own stack) | same DX, on ratatui's buffer model |

## Quick start

```rust
use ratatui_css::{CssStyle, Origin, OwnedNode, Stylesheet};

let mut sheet = Stylesheet::new();
sheet.add(
    "Button.primary",
    CssStyle::new().color("#fff").background("blue").bold(),
    Origin::User,
)?;

let node = OwnedNode::new("Button").with_classes(["primary"]);
let computed = sheet.compute(&node, None);

// Project onto native ratatui:
let _style = computed.to_style();
let _block = computed.to_block();
```

## CSS text stylesheets

```rust
use ratatui_css::Stylesheet;

let sheet = Stylesheet::parse(r#"
    :root { --accent: #00d4ff; }

    Button.primary {
        color: var(--accent);
        background: blue;
        font-weight: bold;
        border: rounded;
        padding: 0 2;
    }
    Button:focus { background: green; }
    #save:disabled { opacity: dim; }
"#)?;
```

## Inheritance & `var()`

Color, font-style, text-decoration, and text-align inherit from the parent's
computed style. `var(--name)` resolves against the `:root` token table (or a
`ThemeTokens` built programmatically / from themekit).

## Integration with a framework

Implement `StyledNode` on your node type — the engine knows nothing about your
framework:

```rust
use ratatui_css::{StyledNode, State, Position};

impl StyledNode for MyNode {
    fn type_name(&self) -> &str { &self.kind }
    fn id(&self) -> Option<&str> { self.id.as_deref() }
    fn classes(&self) -> Vec<&str> { self.classes.iter().map(String::as_str).collect() }
    fn state(&self) -> State { self.state }
    fn position(&self) -> Position { self.position.clone() }
}
```

## Features

| Feature | Default | Adds |
|---|---|---|
| `serde` | ✅ | `Serialize`/`Deserialize` for all value types (JSON property maps) |
| `themekit` | ❌ | `ThemeTokens::from_themekit` — bridge `ratatui-themekit` slots to CSS variables |

Disable default features for a pure, zero-dep style engine.

## Status

Implemented through **P2** (cascade + selectors + specificity + pseudo-states +
inheritance + `var()` tokens + themekit interop). **P3** (descendant/child
combinators, `:nth-child`, `@media`, `ComputedStyle` cache) is future work.

## License

MIT
