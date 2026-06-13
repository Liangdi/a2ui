# RFC: ratatui-css — A CSS Cascade Engine for ratatui

| | |
|---|---|
| **Status** | Implemented through P2 (inheritance + `var()` tokens + themekit interop) |
| **Version** | 0.1.0 |
| **Authors** | Liangdi `<wu@liangdi.me>` |
| **Depends on** | `ratatui` 0.30 (core types: `Style` / `Color` / `Modifier` / `Rect` / `Block`) |

## 1. Motivation

ratatui's styling primitives — `Style`, `Modifier`, `Color`, `Block` — are powerful but **imperative
and local**: every widget hand-assembles its own style at the call site. There is no shared layer
for:

- **Declarative, data-driven style** (a style that comes from JSON/TOML config or, in
  server-driven UIs like a2ui, from the wire).
- **Reuse across widgets** (a "primary button" look defined once).
- **Cascade & specificity** (type vs. class vs. id precedence).
- **Inheritance** (a parent's text color reaching its children — ratatui does not inherit).
- **Pseudo-states** (`:focus`, `:disabled`) wired into a single resolution pass.

### Ecosystem gap

| Existing crate | Provides | Missing vs. this RFC |
|---|---|---|
| `ratatui-themekit` | 15 semantic color slots + 11 themes + widget builders | No CSS property names, no cascade, no selectors, no pseudo-class rules |
| `ratatui-themes` | Color-theme collection | Same |
| `tui-theme-builder` | Declarative **macro** → `Style` | Compile-time only; not runtime/config-driven |
| `lipgloss` (Go, Charm) | The "CSS for terminals" DX | Own rendering stack — not ratatui's buffer model |

**Position:** `ratatui-css` is the *cascade layer* — it composes with `ratatui-themekit` (which
provides "where colors come from") rather than competing with it, and it brings lipgloss-style
declarative DX onto ratatui's native `Style`/`Block`.

## 2. Goals / Non-Goals

**Goals**

- Speak CSS property names (`color`, `background-color`, `font-weight`, `border`, `padding`,
  `margin`, `text-align`, `width`, `height`, …).
- Be `serde`-serializable: JSON/TOML/YAML in, `Style`/`Block` out.
- Implement a pragmatic cascade: **origin × specificity × inheritance × pseudo-states**.
- Stay framework-agnostic: the engine knows a minimal `StyledNode` trait, not a2ui, not any
  widget.
- Produce **native ratatui values** — never a parallel rendering stack.

**Non-Goals (v1)**

- Pixel box model (terminal is a character grid; units are *cells*).
- CSS animations / transitions (future: state timeline).
- A layout engine (ratatui + the host's layout own that; we only emit `Constraint`s).
- Full selector combinator coverage (descendant/child/sibling are P3).

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Host: a2ui SurfaceRenderer  /  any ratatui application      │
│        ↓ supplies a StyledNode (type / id / classes / state) │
│  Cascade Engine   (origin × specificity × inheritance × :…) │
│        ↓ ComputedStyle                                       │
│  Stylesheet       (Rule = selector + declarations)           │
│        ↓                                                     │
│  Value Types      (CssStyle / Color / BoxModel)              │
│        ↓ to_style() / to_block() / apply_margin() / …        │
│  ratatui: Style, Modifier, Color, Rect, Block, Constraint    │
└─────────────────────────────────────────────────────────────┘
```

## 4. Core Abstractions

### 4.1 `Color` — CSS color syntax + token reference

```text
Color ::= Literal(ratatui::style::Color)   // #rgb | #rrggbb | rgb() | named
        | Var("--name")                    // var(--accent)  → resolved against ThemeTokens
        | Inherit                          // take from parent ComputedStyle
        | Reset                            // transparent / none / Color::Reset
```

Parsing supports `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb(r,g,b)`, `rgba(r,g,b,a)`, the CSS
basic + extended named colors (mapped to ratatui named colors where they exist, else `Rgb`),
`transparent`/`none`→`Reset`, `inherit`, and `var(--name)`.

### 4.2 `CssStyle` — a declaration block

A `CssStyle` is a set of *optional* declarations. `None` means "not set" — which is exactly the
cascade's "do not override" signal and mirrors ratatui's own `Option`-based `Style`.

It groups three concerns, each with a ratatui projection:

| Group | Fields | Projection |
|---|---|---|
| Decoration | `color`, `background`, `weight`, `style`, `decoration`, `underline_color` | `to_style() → ratatui::Style` |
| Box model | `padding`, `margin`, `border` | `to_block() → Block` / `apply_margin(Rect) → Rect` |
| Sizing | `text_align`, `width`, `height` | `constraints() → (Constraint, Constraint)` / `Alignment` |

### 4.3 `StyledNode` — framework-agnostic element view

```rust
pub trait StyledNode {
    fn type_name(&self) -> &str;
    fn id(&self) -> Option<&str>;
    fn classes(&self) -> &[&str];
    fn state(&self) -> State;
    fn position(&self) -> Position;
}
```

`State` is `{ focus, hover, disabled, checked, active }`; `Position` is
`{ index, sibling_count, parent_type }` (used by future `:nth-child`). a2ui implements this on
`ComponentModel`; a vanilla ratatui app implements it on its own state structs.

### 4.4 `Selector` — pragmatic CSS subset

```ebnf
selector      ::= compound ( "," compound )*        (* comma = multiple rules *)
compound      ::= ( type_name )? ( "." class | "#" id | ":" pseudo )+
type_name     ::= IDENT
pseudo        ::= "focus" | "hover" | "disabled" | "checked" | "active"
```

**Specificity** (CSS tuple, compared lexicographically):

```
(a = #ids,  b = #classes + #pseudos,  c = type_name ? 1 : 0)
```

### 4.5 `Stylesheet` + `Rule` + `Origin`

```rust
pub enum Origin { UserAgent, Theme, User, Inline }   // later overrides earlier

pub struct Rule  { selector, style: CssStyle, origin, source_order }
pub struct Stylesheet { rules: Vec<Rule>, tokens: ThemeTokens }
```

Origin precedence: `UserAgent < Theme < User < Inline`.

### 4.6 `ThemeTokens` — CSS custom properties (P2)

A map of variable names (without the `--`) to `Color`. Populated from config or from a semantic
source. `var(--x)` is resolved against this map at compute time, recursively, with a cycle guard.

### 4.7 `ComputedStyle` — the fully resolved result

Holds a `CssStyle` whose `Color` fields have all been resolved to `Literal`, and whose inheritable
fields have been filled from the parent. Exposes `to_style()`, `to_block()`, `apply_margin()`,
`constraints()`, `alignment()`.

## 5. Cascade Algorithm

```
compute(node, parent):
    matching = [ r for r in stylesheet.rules if r.selector.matches(node) ]
    sort matching ascending by (origin_rank, specificity, source_order)

    own = CssStyle::empty()
    for r in matching:               # later (higher priority) overlays earlier
        own.overlay(r.style)

    own.resolve_vars(stylesheet.tokens, parent_tokens)   # P2: var() → Literal
    own.inherit_from(parent)          # P2: fill unset inheritable fields from parent
    return ComputedStyle(own)
```

### 5.1 Inheritance

Inheritable fields: `color`, `weight`, `style`, `decoration`, `underline_color`, `text_align`.
Non-inheritable (reset to default when unset): `background`, `border`, `padding`, `margin`,
`width`, `height`.

This is a real improvement over native ratatui, which has no inheritance at all.

### 5.2 `var()` resolution (P2)

`Color::Var(name)` resolves through `ThemeTokens`. Variables may reference other variables; the
resolver walks the chain with a depth cap and cycle guard, falling back to `parent_tokens`.

### 5.3 themekit interop (P2)

`ThemeTokens::from_themekit(&theme)` seeds the standard semantic slots as CSS variables
(`--accent`, `--text`, `--border`, `--surface`, `--success`, …), mapping themekit's 15-slot
`Theme` trait one-to-one. Stylesheets then write `color: var(--accent)` and the existing themekit
palettes drive the cascade unchanged. Gated behind the `themekit` feature.

## 6. ratatui Mapping

| CSS | ratatui output | Notes |
|---|---|---|
| `color` | `Style::fg` | |
| `background-color` | `Style::bg` | |
| `font-weight: bold` | `add_modifier(BOLD)` | numeric ≥ 700 also bold |
| `font-style: italic` | `ITALIC` | |
| `text-decoration: underline` | `UNDERLINED` | `line-through` → `CROSSED_OUT` |
| `border` | `Block::borders` + `border_type` + `border_style` | `border-radius`→`Rounded` |
| `padding` | `Block::padding` | units = cells |
| `margin` | `Rect` shrink | |
| `text-align` | `Alignment` | |
| `width` / `height` | `Constraint` | `Length` / `Percentage` / `Min` / `Max` |
| `opacity` | `DIM` | on/off only |

## 7. Adoption Levels

- **L0 — Value types only.** Use `CssStyle` + `Color` parser to build `Style`/`Block` from config.
  No cascade. Replaces `tui-theme-builder` for runtime config; the minimum for server-driven UI.
- **L1 — Stylesheet + class.** Parse a stylesheet once; resolve by class/id at render. The sweet
  spot.
- **L2 — Full cascade.** Register a `Stylesheet`, pass `StyledNode`s, get `ComputedStyle`s with
  inheritance, specificity, and pseudo-states.

## 8. Crate Layout & Dependencies

```
ratatui-css/
├── design.md         (this RFC)
├── src/
│   ├── lib.rs        re-exports
│   ├── error.rs      CssError (std-only, no thiserror)
│   ├── color.rs      P0  Color + parser
│   ├── style.rs      P0  CssStyle declaration block
│   ├── box_model.rs  P0  padding/margin/border → Block/Rect/Constraint
│   ├── node.rs       P1  StyledNode / State / Position
│   ├── selector.rs   P1  Selector parse + match + specificity
│   ├── stylesheet.rs P1  Rule / Origin / Stylesheet (+ text parser)
│   ├── token.rs      P2  ThemeTokens + var() resolution
│   ├── cascade.rs    P1+P2  engine + ComputedStyle + inheritance
│   └── themekit.rs   P2  feature `themekit` — Theme::from_themekit
```

Hard dependency: `ratatui` 0.30. Optional: `serde` (default), `serde_json` (`json`),
`ratatui-themekit` (`themekit`). The error type is hand-rolled to avoid a `thiserror` dependency.

## 9. Performance

- Stylesheets parse once; rules are pre-sorted by specificity.
- Per-element matching is O(rules) with cheap predicate checks; no DOM walk for simple selectors.
- `var()` resolution memoizes within a compute; cycle-guarded.
- (P3, future) `ComputedStyle` LRU cache keyed by element signature, invalidated on state change.

## 10. Roadmap

| Phase | Scope | Status |
|---|---|---|
| **P0** | `CssStyle` + `Color` parser + serde + `to_style/to_block/apply_margin/constraints` | ✅ Done |
| **P1** | `Stylesheet` + selectors (type/class/id/compound) + specificity cascade + `:focus`/`:disabled`/`:checked`/`:hover`/`:active` + `StyledNode` | ✅ Done |
| **P2** | inheritance + `var()` tokens + themekit interop | ✅ Done |
| **P3** | descendant/child combinators, `:nth-child`, `@media` (terminal size + color capability), `ComputedStyle` cache | ☐ Future |

## 11. Open Questions (P3+)

- Should `@media` queries key off terminal cell size, `$COLUMNS`/`$LINES`, or color capability
  (`NO_COLOR`, truecolor detection)? Likely all three.
- Descendant combinators require an ancestor stack threaded through the tree walk — decide the
  host-facing API (explicit `compute(node, parent)` vs. an engine that owns the walk).
- Whether to ship a `css!{ … }` compile-time DSL (prefer `macro_rules!` to avoid a proc-macro
  dependency).
