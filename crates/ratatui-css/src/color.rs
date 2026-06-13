//! CSS color syntax → ratatui `Color`.
//!
//! Supports `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa`, `rgb()` / `rgba()`,
//! the CSS named colors (mapped to ratatui named colors where possible, else
//! `Rgb`), `transparent` / `none` / `reset`, `inherit`, and `var(--name)`.
//!
//! `var()` references are *not* resolved here — they are kept as [`Color::Var`]
//! and resolved against a [`crate::token::ThemeTokens`] table during the
//! cascade (see `token.rs`).

use ratatui::style::Color as RColor;

use crate::error::{CssError, Result};

/// A CSS color value.
///
/// `Var` and `Inherit` are only meaningful during the cascade; after
/// [`crate::cascade::ComputedStyle`] resolution every `Color` is `Literal`,
/// `Reset`, or (if still unresolved) left as-is.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Color {
    /// A concrete ratatui color.
    Literal(RColor),
    /// A reference to a CSS custom property: `var(--name)` with an optional fallback.
    Var {
        name: String,
        fallback: Option<Box<Color>>,
    },
    /// Take the value from the parent's computed style (inheritable properties only).
    Inherit,
    /// `Color::Reset` — transparent / none / terminal default. (The default.)
    #[default]
    Reset,
}

impl Color {
    /// Shortcut for a literal ratatui color.
    pub const fn literal(c: RColor) -> Self {
        Self::Literal(c)
    }

    /// Shortcut for `var(--name)` with no fallback.
    pub fn var(name: impl Into<String>) -> Self {
        Self::Var { name: name.into(), fallback: None }
    }

    /// Parse a CSS color expression.
    pub fn parse(input: &str) -> Result<Self> {
        let s = input.trim();
        let lower = s.to_ascii_lowercase();

        match lower.as_str() {
            "inherit" | "currentcolor" => return Ok(Self::Inherit),
            "transparent" | "none" | "reset" | "initial" => return Ok(Self::Reset),
            _ => {}
        }

        if let Some(rest) = lower.strip_prefix("var(") {
            return Self::parse_var(rest);
        }
        if let Some(rest) = lower.strip_prefix('#') {
            let c = parse_hex(rest).ok_or_else(|| CssError::InvalidColor(s.into()))?;
            return Ok(literal_or_reset(c));
        }
        if let Some(rest) = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")) {
            let c = parse_rgb(rest).ok_or_else(|| CssError::InvalidColor(s.into()))?;
            return Ok(literal_or_reset(c));
        }
        if let Some(c) = named_color(&lower) {
            return Ok(Self::Literal(c));
        }

        Err(CssError::InvalidColor(s.into()))
    }

    fn parse_var(inner: &str) -> Result<Self> {
        // inner still has a trailing ')'.
        let inner = inner.trim();
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let (name_part, fallback_part) = split_top_comma(inner);
        let name = name_part.trim().trim_start_matches('-').trim().to_string();
        if name.is_empty() {
            return Err(CssError::InvalidColor(format!("var(): empty name in {inner}")));
        }
        let fallback = match fallback_part.trim() {
            "" => None,
            expr => Some(Box::new(Self::parse(expr)?)),
        };
        Ok(Self::Var { name, fallback })
    }

    /// Returns `true` if this is a `var()` reference that needs token resolution.
    pub fn is_var(&self) -> bool {
        matches!(self, Self::Var { .. })
    }
}

/// Parse a CSS color string into a [`Color`]. Invalid expressions degrade to
/// [`Color::Reset`] rather than panicking — consistent with the cascade's
/// lenient resolution (unresolved `var()`s also degrade to `Reset`).
impl From<&str> for Color {
    fn from(s: &str) -> Self {
        Color::parse(s).unwrap_or(Self::Reset)
    }
}

impl From<String> for Color {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Literal(c) => write!(f, "{}", format_literal(c)),
            Self::Var { name, fallback } => match fallback {
                Some(fb) => write!(f, "var(--{name}, {fb})"),
                None => write!(f, "var(--{name})"),
            },
            Self::Inherit => f.write_str("inherit"),
            Self::Reset => f.write_str("transparent"),
        }
    }
}

/// Wrap a ratatui color, collapsing `RColor::Reset` to the `Color::Reset`
/// variant (so transparent expressions like `#0000` and `transparent` are
/// representationally identical).
fn literal_or_reset(c: RColor) -> Color {
    match c {
        RColor::Reset => Color::Reset,
        other => Color::Literal(other),
    }
}

/// Split on the first comma that is not nested inside parentheses.
fn split_top_comma(s: &str) -> (&str, &str) {
    let mut depth: u32 = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return (&s[..i], &s[i + 1..]),
            _ => {}
        }
    }
    (s, "")
}

fn parse_hex(hex: &str) -> Option<RColor> {
    let hex = hex.trim();
    match hex.len() {
        3 => rgb_from_hex(&format!(
            "{x}{x}{y}{y}{z}{z}",
            x = &hex[0..1],
            y = &hex[1..2],
            z = &hex[2..3]
        )),
        4 => {
            // #rgba — alpha == 0 means transparent.
            if &hex[3..4] == "0" {
                Some(RColor::Reset)
            } else {
                parse_hex(&hex[0..3])
            }
        }
        6 => rgb_from_hex(hex),
        8 => {
            if &hex[6..8] == "00" {
                Some(RColor::Reset)
            } else {
                parse_hex(&hex[0..6])
            }
        }
        _ => None,
    }
}

fn rgb_from_hex(hex: &str) -> Option<RColor> {
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(RColor::Rgb(r, g, b))
}

fn parse_rgb(inner: &str) -> Option<RColor> {
    // inner still has a trailing ')'. Collect integer components.
    let inner = inner.trim().strip_suffix(')').unwrap_or(inner);
    let mut nums: Vec<u8> = Vec::new();
    for tok in inner.split(|c: char| c == ',' || c.is_whitespace() || c == '/') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        // rgb() may use percentages: rgb(100%, 0%, 0%).
        let n = if let Some(pct) = tok.strip_suffix('%') {
            (pct.parse::<f32>().ok()? / 100.0 * 255.0).round() as u8
        } else {
            // tolerate "0.5" alpha — only integer rgb channels matter.
            tok.parse::<u8>().ok().or_else(|| tok.split('.').next().and_then(|s| s.parse::<u8>().ok()))?
        };
        nums.push(n);
    }
    let r = *nums.first()?;
    let g = *nums.get(1)?;
    let b = *nums.get(2)?;
    // If a 4th component (alpha) is present and zero, treat as transparent.
    if nums.len() == 4 && nums[3] == 0 {
        return Some(RColor::Reset);
    }
    // Explicit rgb()/hex always yields an Rgb literal; named colors come only
    // from the named-keyword path. This keeps the two representations distinct
    // and predictable.
    Some(RColor::Rgb(r, g, b))
}

/// A curated set of CSS named colors mapped to ratatui colors (named where
/// possible, else the canonical sRGB value as `Rgb`).
fn named_color(name: &str) -> Option<RColor> {
    Some(match name {
        // CSS basic 16
        "black" => RColor::Black,
        "white" => RColor::White,
        "gray" | "grey" => RColor::Gray,
        "silver" | "lightgray" | "lightgrey" => RColor::DarkGray,
        "darkgray" | "darkgrey" => RColor::Gray,
        "red" => RColor::Red,
        "darkred" => RColor::Rgb(128, 0, 0),
        "maroon" => RColor::Rgb(128, 0, 0),
        "lightred" => RColor::LightRed,
        "green" => RColor::Green,
        "darkgreen" => RColor::Rgb(0, 100, 0),
        "lime" => RColor::LightGreen,
        "lightgreen" => RColor::LightGreen,
        "olive" => RColor::Rgb(128, 128, 0),
        "yellow" => RColor::Yellow,
        "lightyellow" => RColor::LightYellow,
        "blue" => RColor::Blue,
        "navy" => RColor::Rgb(0, 0, 128),
        "lightblue" => RColor::LightBlue,
        "teal" | "cyan" | "aqua" => RColor::Cyan,
        "lightcyan" => RColor::LightCyan,
        "purple" => RColor::Rgb(128, 0, 128),
        "magenta" | "fuchsia" => RColor::Magenta,
        "lightmagenta" | "pink" => RColor::LightMagenta,
        // ratatui-specific conveniences
        "dim" | "darkgray-ratui" => RColor::DarkGray,
        // A few popular extended colors (as Rgb)
        "orange" => RColor::Rgb(255, 165, 0),
        "brown" => RColor::Rgb(165, 42, 42),
        "gold" => RColor::Rgb(255, 215, 0),
        "indigo" => RColor::Rgb(75, 0, 130),
        "violet" => RColor::Rgb(238, 130, 238),
        "crimson" => RColor::Rgb(220, 20, 60),
        "salmon" => RColor::Rgb(250, 128, 114),
        "coral" => RColor::Rgb(255, 127, 80),
        "turquoise" => RColor::Rgb(64, 224, 208),
        "slategray" | "slategrey" => RColor::Rgb(112, 128, 144),
        _ => return None,
    })
}

fn format_literal(c: &RColor) -> String {
    match c {
        RColor::Reset => "transparent".into(),
        RColor::Black => "black".into(),
        RColor::Red => "red".into(),
        RColor::Green => "green".into(),
        RColor::Yellow => "yellow".into(),
        RColor::Blue => "blue".into(),
        RColor::Magenta => "magenta".into(),
        RColor::Cyan => "cyan".into(),
        RColor::Gray => "gray".into(),
        RColor::DarkGray => "darkgray".into(),
        RColor::LightRed => "lightred".into(),
        RColor::LightGreen => "lightgreen".into(),
        RColor::LightYellow => "lightyellow".into(),
        RColor::LightBlue => "lightblue".into(),
        RColor::LightMagenta => "lightmagenta".into(),
        RColor::LightCyan => "lightcyan".into(),
        RColor::White => "white".into(),
        RColor::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        RColor::Indexed(i) => format!("indexed({i})"),
    }
}

// ---------------------------------------------------------------------------
// Optional serde support — (de)serialize as a CSS color string.
// ---------------------------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_impl {
    use super::Color;

    impl<'de> serde::Deserialize<'de> for Color {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let s = String::deserialize(d)?;
            Color::parse(&s).map_err(serde::de::Error::custom)
        }
    }

    impl serde::Serialize for Color {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(&self.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_3_4_6_8() {
        assert_eq!(Color::parse("#fff").unwrap(), Color::literal(RColor::Rgb(255, 255, 255)));
        assert_eq!(Color::parse("#0000").unwrap(), Color::Reset); // alpha 0
        assert_eq!(Color::parse("#ff8800").unwrap(), Color::literal(RColor::Rgb(255, 136, 0)));
        assert_eq!(Color::parse("#ff880000").unwrap(), Color::Reset);
        assert_eq!(Color::parse("#ff8800ff").unwrap(), Color::literal(RColor::Rgb(255, 136, 0)));
    }

    #[test]
    fn rgb_fn() {
        assert_eq!(Color::parse("rgb(0,255,0)").unwrap(), Color::literal(RColor::Rgb(0, 255, 0)));
        assert_eq!(Color::parse("rgb(1 2 3)").unwrap(), Color::literal(RColor::Rgb(1, 2, 3)));
        assert_eq!(Color::parse("rgba(10,20,30,0)").unwrap(), Color::Reset);
    }

    #[test]
    fn named() {
        assert_eq!(Color::parse("red").unwrap(), Color::literal(RColor::Red));
        assert_eq!(Color::parse("CYAN").unwrap(), Color::literal(RColor::Cyan));
        assert_eq!(Color::parse("orange").unwrap(), Color::literal(RColor::Rgb(255, 165, 0)));
    }

    #[test]
    fn keywords() {
        assert_eq!(Color::parse("transparent").unwrap(), Color::Reset);
        assert_eq!(Color::parse("inherit").unwrap(), Color::Inherit);
    }

    #[test]
    fn var_refs() {
        match Color::parse("var(--accent)") {
            Ok(Color::Var { name, fallback: None }) => assert_eq!(name, "accent"),
            other => panic!("{other:?}"),
        }
        match Color::parse("var(--text, #fff)") {
            Ok(Color::Var { name, fallback: Some(fb) }) => {
                assert_eq!(name, "text");
                assert_eq!(*fb, Color::literal(RColor::Rgb(255, 255, 255)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn invalid() {
        assert!(Color::parse("#zzz").is_err());
        assert!(Color::parse("banana").is_err());
    }
}
