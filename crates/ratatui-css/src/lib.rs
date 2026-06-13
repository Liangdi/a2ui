//! # ratatui-css
//!
//! A CSS cascade engine for ratatui — selectors, specificity, inheritance,
//! pseudo-states, and data-driven styling. Produces native ratatui `Style` /
//! `Block` / `Constraint` values; it is never a parallel rendering stack.
//!
//! See [`design.md`](https://github.com/Liangdi/a2ui/blob/master/crates/ratatui-css/design.md)
//! for the full RFC.
//!
//! # Quick start (L1 — stylesheet + class)
//!
//! ```
//! use ratatui_css::{CssStyle, Origin, OwnedNode, Stylesheet};
//!
//! let mut sheet = Stylesheet::new();
//! sheet.add(
//!     "Button.primary",
//!     CssStyle::new().color("#fff").background("blue").bold(),
//!     Origin::User,
//! )
//! .unwrap();
//!
//! let node = OwnedNode::new("Button").with_classes(["primary"]);
//! let computed = sheet.compute(&node, None);
//! let _ratatui_style = computed.to_style();
//! ```

pub mod box_model;
pub mod cascade;
pub mod color;
pub mod error;
pub mod node;
pub mod selector;
pub mod style;
pub mod stylesheet;
pub mod token;

#[cfg(feature = "themekit")]
pub mod themekit;

// Re-exports — the primary public surface.
pub use box_model::{BorderSpec, BorderStyle, BoxEdges, Length};
pub use cascade::ComputedStyle;
pub use color::Color;
pub use error::{CssError, Result};
pub use node::{OwnedNode, Position, State, StyledNode};
pub use selector::{PseudoClass, Selector};
pub use style::{Align, CssStyle, FontStyle, TextDecoration, Weight};
pub use stylesheet::{apply_decl, Origin, RuleEntry, Stylesheet};
pub use token::ThemeTokens;
