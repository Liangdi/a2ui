//! CSS custom properties — the `var()` resolution target.
//!
//! A [`ThemeTokens`] table maps variable names (without the `--` prefix) to
//! [`Color`] values. `var(--name)` references in a [`crate::style::CssStyle`]
//! are resolved against this table during the cascade (see `cascade.rs`).

use std::collections::HashMap;

use ratatui::style::Color as RColor;

use crate::color::Color;
use crate::error::{CssError, Result};

/// A map of CSS custom-property names to colors.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeTokens {
    vars: HashMap<String, Color>,
}

impl ThemeTokens {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert/overwrite a variable. `name` is given without the `--` prefix.
    pub fn set(mut self, name: impl Into<String>, color: impl Into<Color>) -> Self {
        self.vars.insert(name.into(), color.into());
        self
    }

    /// Insert/overwrite a variable (mutable).
    pub fn insert(&mut self, name: impl Into<String>, color: impl Into<Color>) {
        self.vars.insert(name.into(), color.into());
    }

    /// Look up a variable by name (without `--`).
    pub fn get(&self, name: &str) -> Option<&Color> {
        self.vars.get(name)
    }

    /// Merge another token set into this one; `other` wins on conflict.
    pub fn merge(&mut self, other: &ThemeTokens) {
        for (k, v) in &other.vars {
            self.vars.insert(k.clone(), v.clone());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    pub fn len(&self) -> usize {
        self.vars.len()
    }
}

/// Resolve a `var()` reference chain to a concrete ratatui color.
///
/// - `Literal` / `Reset` map straight through.
/// - `Var` is looked up in `tokens`; if absent, the `var()` fallback is used;
///   if there is no fallback, returns [`CssError::UndefinedVariable`].
/// - `Inherit` resolves to `Reset` (it should have been folded in by the
///   inheritance pass already).
/// - Cycles / chains deeper than 32 return [`CssError::CircularVariable`].
pub fn resolve_strict(color: &Color, tokens: &ThemeTokens) -> Result<RColor> {
    resolve_inner(color, tokens, 0)
}

/// Lenient variant used by the cascade: unresolved variables degrade to
/// `Reset` rather than failing the whole render.
pub fn resolve(color: &Color, tokens: &ThemeTokens) -> RColor {
    resolve_strict(color, tokens).unwrap_or(RColor::Reset)
}

fn resolve_inner(color: &Color, tokens: &ThemeTokens, depth: u8) -> Result<RColor> {
    if depth > 32 {
        return Err(CssError::CircularVariable(
            "var() reference chain too deep (depth > 32)".to_string(),
        ));
    }
    match color {
        Color::Literal(c) => Ok(*c),
        Color::Reset => Ok(RColor::Reset),
        Color::Inherit => Ok(RColor::Reset),
        Color::Var { name, fallback } => match tokens.get(name) {
            Some(referent) => resolve_inner(referent, tokens, depth + 1),
            None => match fallback {
                Some(fb) => resolve_inner(fb, tokens, depth + 1),
                None => Err(CssError::UndefinedVariable(name.clone())),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_simple_var() {
        let tokens = ThemeTokens::new().set("accent", Color::literal(RColor::Blue));
        let c = Color::var("accent");
        assert_eq!(resolve_strict(&c, &tokens).unwrap(), RColor::Blue);
    }

    #[test]
    fn resolves_chain() {
        let tokens = ThemeTokens::new()
            .set("accent", Color::var("blue"))
            .set("blue", Color::literal(RColor::Blue));
        assert_eq!(resolve_strict(&Color::var("accent"), &tokens).unwrap(), RColor::Blue);
    }

    #[test]
    fn uses_fallback() {
        let tokens = ThemeTokens::new();
        let c = Color::Var { name: "missing".into(), fallback: Some(Box::new(Color::literal(RColor::Red))) };
        assert_eq!(resolve_strict(&c, &tokens).unwrap(), RColor::Red);
    }

    #[test]
    fn undefined_is_error_strict_but_reset_lenient() {
        let tokens = ThemeTokens::new();
        assert!(resolve_strict(&Color::var("nope"), &tokens).is_err());
        assert_eq!(resolve(&Color::var("nope"), &tokens), RColor::Reset);
    }
}
