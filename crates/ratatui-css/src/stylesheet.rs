//! Stylesheet — a collection of `Rule`s plus a token table, with a small CSS
//! text parser and a builder API.
//!
//! A stylesheet is parsed once and queried many times by the cascade engine.

use crate::color::Color;
use crate::error::{CssError, Result};
use crate::selector::Selector;
use crate::style::{Align, CssStyle, FontStyle, TextDecoration, Weight};
use crate::token::ThemeTokens;

/// Cascade origin. Later origins override earlier ones at equal specificity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Origin {
    /// Built-in defaults.
    #[default]
    UserAgent,
    /// Application-wide theme.
    Theme,
    /// End-user configuration (e.g. a config file).
    User,
    /// Inline declaration on the element itself — highest priority.
    Inline,
}

/// A flattened rule: one selector + one declaration block + provenance.
#[derive(Debug, Clone)]
pub struct RuleEntry {
    pub selector: Selector,
    pub style: CssStyle,
    pub origin: Origin,
    /// Insertion order within the stylesheet — the CSS "source order" tiebreaker.
    pub order: usize,
}

/// A parsed stylesheet.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    rules: Vec<RuleEntry>,
    tokens: ThemeTokens,
}

impl Stylesheet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with a token table (CSS custom properties).
    pub fn with_tokens(tokens: ThemeTokens) -> Self {
        Self { rules: Vec::new(), tokens }
    }

    pub fn tokens(&self) -> &ThemeTokens {
        &self.tokens
    }
    pub fn tokens_mut(&mut self) -> &mut ThemeTokens {
        &mut self.tokens
    }
    pub fn rules(&self) -> &[RuleEntry] {
        &self.rules
    }

    /// Add a rule from a selector string (may be a comma list) + style.
    pub fn add(&mut self, selectors: &str, style: CssStyle, origin: Origin) -> Result<&mut Self> {
        let order_base = self.rules.len();
        for sel in Selector::parse_list(selectors)? {
            self.rules.push(RuleEntry { selector: sel, style: style.clone(), origin, order: order_base });
        }
        Ok(self)
    }

    /// Add a single pre-parsed rule.
    pub fn add_rule(&mut self, selector: Selector, style: CssStyle, origin: Origin) -> &mut Self {
        let order = self.rules.len();
        self.rules.push(RuleEntry { selector, style, origin, order });
        self
    }

    /// Merge another stylesheet's rules and tokens into this one.
    pub fn extend(&mut self, other: &Stylesheet) {
        self.tokens.merge(&other.tokens);
        let offset = self.rules.len();
        for r in &other.rules {
            self.rules.push(RuleEntry { order: offset + r.order, ..r.clone() });
        }
    }

    /// Parse a CSS text document.
    ///
    /// Supports `selector { prop: value; … }` blocks, comma selector lists,
    /// the universal `*`, `:root { --name: color; }` for tokens, and `/* … */`
    /// comments. Declarations use [`Stylesheet::apply_decl`]'s property names.
    pub fn parse(css: &str) -> Result<Self> {
        let cleaned = strip_comments(css);
        let mut sheet = Stylesheet::new();
        let mut rest = cleaned.as_str();

        while let Some(brace) = rest.find('{') {
            let selector_part = rest[..brace].trim();
            rest = &rest[brace + 1..];
            let close = rest.find('}').ok_or_else(|| {
                CssError::InvalidSelector("missing closing `}`".into())
            })?;
            let body = &rest[..close];
            rest = &rest[close + 1..];

            if selector_part.is_empty() {
                return Err(CssError::InvalidSelector("rule with no selector".into()));
            }

            // `:root { --x: … }` declares tokens.
            let is_root = selector_part.split(',').all(|s| s.trim() == ":root");
            if is_root {
                for (prop, value) in split_declarations(body) {
                    if let Some(name) = prop.strip_prefix("--") {
                        sheet.tokens.insert(name.trim(), Color::parse(value)?);
                    }
                }
                continue;
            }

            let mut style = CssStyle::new();
            for (prop, value) in split_declarations(body) {
                let prop = prop.trim();
                let value = value.trim();
                if prop.is_empty() {
                    continue;
                }
                if let Some(name) = prop.strip_prefix("--") {
                    sheet.tokens.insert(name, Color::parse(value)?);
                } else {
                    apply_decl(&mut style, prop, value)?;
                }
            }
            // Text-parsed rules default to the User origin.
            sheet.add(selector_part, style, Origin::User)?;
        }

        Ok(sheet)
    }
}

/// Apply one `prop: value` declaration to a [`CssStyle`] (text form).
pub fn apply_decl(style: &mut CssStyle, prop: &str, value: &str) -> Result<()> {
    let prop = prop.trim().to_ascii_lowercase();
    match prop.as_str() {
        "color" => style.color = Some(Color::parse(value)?),
        "background" | "background-color" => style.background = Some(Color::parse(value)?),
        "font-weight" => style.weight = Some(parse_weight(value)?),
        "font-style" => style.font_style = Some(parse_font_style(value)?),
        "text-decoration" => style.decoration = Some(parse_decoration(value)?),
        "underline-color" => style.underline_color = Some(Color::parse(value)?),
        "padding" => style.padding = Some(crate::box_model::BoxEdges::parse(value)?),
        "margin" => style.margin = Some(crate::box_model::BoxEdges::parse(value)?),
        "border" => style.border = Some(crate::box_model::BorderSpec::parse_shorthand(value)?),
        "text-align" => style.text_align = Some(parse_align(value)?),
        "width" => style.width = Some(crate::box_model::Length::parse(value)?),
        "height" => style.height = Some(crate::box_model::Length::parse(value)?),
        _ => { /* unknown property → ignored (forward-compat) */ }
    }
    Ok(())
}

fn parse_weight(v: &str) -> Result<Weight> {
    match v.trim().to_ascii_lowercase().as_str() {
        "bold" | "bolder" => Ok(Weight::Bold),
        "normal" | "lighter" | "" => Ok(Weight::Normal),
        other => other
            .parse::<u32>()
            .map(|n| if n >= 600 { Weight::Bold } else { Weight::Normal })
            .map_err(|_| CssError::InvalidLength(format!("font-weight: {v}"))),
    }
}

fn parse_font_style(v: &str) -> Result<FontStyle> {
    match v.trim().to_ascii_lowercase().as_str() {
        "italic" | "oblique" => Ok(FontStyle::Italic),
        "normal" | "" => Ok(FontStyle::Normal),
        other => Err(CssError::InvalidSelector(format!("font-style: {other}"))),
    }
}

fn parse_decoration(v: &str) -> Result<TextDecoration> {
    let lower = v.trim().to_ascii_lowercase();
    let u = lower.split_whitespace().any(|t| t == "underline");
    let l = lower.split_whitespace().any(|t| t == "line-through" || t == "strikethrough");
    Ok(match (u, l) {
        (false, false) => TextDecoration::None,
        (true, false) => TextDecoration::Underline,
        (false, true) => TextDecoration::LineThrough,
        (true, true) => TextDecoration::UnderlineLineThrough,
    })
}

fn parse_align(v: &str) -> Result<Align> {
    match v.trim().to_ascii_lowercase().as_str() {
        "left" | "justify" => Ok(Align::Left),
        "center" => Ok(Align::Center),
        "right" => Ok(Align::Right),
        other => Err(CssError::InvalidSelector(format!("text-align: {other}"))),
    }
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // skip to */
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                j += 1;
            }
            i = j + 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Split a rule body into `(property, value)` pairs on `;`, honoring nested
/// parentheses (so `var(--x, rgb(1,2,3))` survives intact).
fn split_declarations(body: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;
    let bytes = body.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => {
                push_decl(&body[start..i], &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    push_decl(&body[start..], &mut out);
    out
}

fn push_decl<'a>(chunk: &'a str, out: &mut Vec<(&'a str, &'a str)>) {
    let chunk = chunk.trim();
    if chunk.is_empty() {
        return;
    }
    if let Some(colon) = chunk.find(':') {
        out.push((&chunk[..colon], &chunk[colon + 1..]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color as RColor;

    #[test]
    fn parse_text_stylesheet() {
        let css = r#"
            :root {
                --accent: #00d4ff;
            }
            /* a comment */
            Button.primary {
                color: var(--accent);
                background: blue;
                font-weight: bold;
            }
            #save:focus { text-decoration: underline; }
        "#;
        let sheet = Stylesheet::parse(css).unwrap();
        assert_eq!(sheet.tokens().get("accent"), Some(&Color::literal(RColor::Rgb(0, 212, 255))));
        // The `var()` reference is kept verbatim in the rule; resolved at compute time.
        let primary = sheet
            .rules()
            .iter()
            .find(|r| r.selector.classes.iter().any(|c| c == "primary"))
            .unwrap();
        assert_eq!(primary.style.color, Some(Color::var("accent")));
        assert!(sheet.rules().iter().any(|r| r.selector.id.as_deref() == Some("save")));
    }

    #[test]
    fn add_flattens_comma_list() {
        let mut sheet = Stylesheet::new();
        sheet.add("Text, .muted, #title", CssStyle::new(), Origin::User).unwrap();
        assert_eq!(sheet.rules().len(), 3);
    }
}
