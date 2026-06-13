//! The cascade engine — turns a [`Stylesheet`] + a [`StyledNode`] into a fully
//! resolved [`ComputedStyle`].
//!
//! Pipeline per element:
//! 1. Collect matching rules.
//! 2. Sort ascending by `(origin, specificity, source_order)`.
//! 3. Fold declarations via [`CssStyle::overlay`] (later = higher priority).
//! 4. Fold explicit `inherit` keywords and auto-inherited properties from the
//!    parent [`ComputedStyle`].
//! 5. Resolve `var()` references against the token table.

use ratatui::{
    layout::{Alignment, Constraint, Rect},
    style::Style as RStyle,
    widgets::Block,
};

use crate::color::Color;
use crate::node::StyledNode;
use crate::style::CssStyle;
use crate::stylesheet::{RuleEntry, Stylesheet};
use crate::token::{self, ThemeTokens};

/// A fully-resolved style: all `var()`s turned into literals, inheritable
/// properties filled from the parent. Project onto ratatui via the delegate
/// methods.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedStyle {
    pub style: CssStyle,
}

impl ComputedStyle {
    pub fn new(style: CssStyle) -> Self {
        Self { style }
    }
    pub fn to_style(&self) -> RStyle {
        self.style.to_style()
    }
    pub fn to_block(&self) -> Block<'_> {
        self.style.to_block()
    }
    pub fn apply_margin(&self, area: Rect) -> Rect {
        self.style.apply_margin(area)
    }
    pub fn constraints(&self) -> Option<(Constraint, Constraint)> {
        self.style.constraints()
    }
    pub fn alignment(&self) -> Option<Alignment> {
        self.style.alignment()
    }
}

impl Stylesheet {
    /// Compute the resolved style for `node`, optionally inheriting from `parent`.
    pub fn compute(&self, node: &dyn StyledNode, parent: Option<&ComputedStyle>) -> ComputedStyle {
        // 1–2. Collect + sort matching rules ascending by priority.
        let mut matching: Vec<&RuleEntry> = self
            .rules()
            .iter()
            .filter(|r| r.selector.matches(node))
            .collect();
        matching.sort_by_key(|r| (r.origin, r.selector.specificity(), r.order));

        // 3. Fold declarations.
        let mut own = CssStyle::new();
        for r in &matching {
            own.overlay(&r.style);
        }

        // 4. Inheritance.
        if let Some(parent) = parent {
            resolve_explicit_inherit(&mut own, &parent.style);
            own.inherit_from(&parent.style);
        }

        // 5. var() resolution against the stylesheet's token table.
        resolve_vars_in_place(&mut own, self.tokens());

        ComputedStyle::new(own)
    }
}

/// Replace explicit `inherit` keyword colors with the parent's value, for all
/// three color fields (CSS `inherit` forces inheritance even for
/// non-inheritable properties like `background`).
fn resolve_explicit_inherit(own: &mut CssStyle, parent: &CssStyle) {
    if matches!(own.color, Some(Color::Inherit)) {
        own.color = parent.color.clone();
    }
    if matches!(own.background, Some(Color::Inherit)) {
        own.background = parent.background.clone();
    }
    if matches!(own.underline_color, Some(Color::Inherit)) {
        own.underline_color = parent.underline_color.clone();
    }
}

/// Resolve every `var()` / leftover `inherit` in the color fields to a literal.
fn resolve_vars_in_place(style: &mut CssStyle, tokens: &ThemeTokens) {
    resolve_color_field(&mut style.color, tokens);
    resolve_color_field(&mut style.background, tokens);
    resolve_color_field(&mut style.underline_color, tokens);
}

fn resolve_color_field(field: &mut Option<Color>, tokens: &ThemeTokens) {
    if let Some(inner) = field {
        match inner {
            Color::Literal(_) | Color::Reset => {} // already concrete
            Color::Var { .. } | Color::Inherit => {
                *field = Some(Color::Literal(token::resolve(inner, tokens)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{OwnedNode, State};
    use crate::stylesheet::Origin;
    use ratatui::style::Color as RC;

    fn sheet() -> Stylesheet {
        let mut s = Stylesheet::with_tokens(
            crate::token::ThemeTokens::new().set("accent", Color::literal(RC::Cyan)),
        );
        // type rule (low specificity)
        s.add("Button", CssStyle::new().color(RC::Gray), Origin::User).unwrap();
        // class rule (higher specificity)
        s.add("Button.primary", CssStyle::new().background(RC::Blue), Origin::User).unwrap();
        // id rule (highest specificity)
        s.add("#save", CssStyle::new().color(RC::Yellow), Origin::User).unwrap();
        // focus pseudo-state
        s.add("Button:focus", CssStyle::new().background(RC::Green), Origin::User).unwrap();
        // var() consumer
        s.add(".accented", CssStyle::new().color(Color::var("accent")), Origin::User).unwrap();
        // inline (origin) overrides specificity
        s
    }

    #[test]
    fn specificity_wins() {
        let s = sheet();
        let n = OwnedNode::new("Button").with_id("save").with_classes(["primary"]);
        let c = s.compute(&n, None);
        // #save (id) wins over Button (type) for color.
        assert_eq!(c.style.color, Some(Color::literal(RC::Yellow)));
        // .primary (class) wins over Button (type) for background.
        assert_eq!(c.style.background, Some(Color::literal(RC::Blue)));
    }

    #[test]
    fn pseudo_state_matches() {
        let s = sheet();
        let n = OwnedNode::new("Button").with_state(State::focus());
        let c = s.compute(&n, None);
        assert_eq!(c.style.background, Some(Color::literal(RC::Green)));
    }

    #[test]
    fn var_resolves_from_tokens() {
        let s = sheet();
        let n = OwnedNode::new("Text").with_classes(["accented"]);
        let c = s.compute(&n, None);
        assert_eq!(c.style.color, Some(Color::literal(RC::Cyan)));
    }

    #[test]
    fn inheritance_from_parent() {
        let s = sheet();
        let parent_node = OwnedNode::new("Button").with_classes(["primary"]);
        let parent = s.compute(&parent_node, None);
        // Child Text has no color of its own; inherits parent's.
        let child = OwnedNode::new("Text");
        let computed = s.compute(&child, Some(&parent));
        assert_eq!(computed.style.color, Some(Color::literal(RC::Gray)));
    }

    #[test]
    fn origin_overrides_specificity() {
        let mut s = Stylesheet::new();
        s.add("Button", CssStyle::new().color(RC::Red), Origin::User).unwrap();
        // Inline origin wins despite identical selector.
        s.add("Button", CssStyle::new().color(RC::Blue), Origin::Inline).unwrap();
        let n = OwnedNode::new("Button");
        let c = s.compute(&n, None);
        assert_eq!(c.style.color, Some(Color::literal(RC::Blue)));
    }
}
