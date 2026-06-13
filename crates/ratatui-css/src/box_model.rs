//! Box-model value types: padding, margin, border, and sizing lengths.
//!
//! These are *descriptors*; they are projected onto ratatui primitives
//! (`Padding`, `Borders`/`BorderType`, `Constraint`) by methods in `style.rs`.

use ratatui::{
    layout::Constraint,
    widgets::{BorderType, Borders, Padding},
};

use crate::color::Color;
use crate::error::{CssError, Result};

// ---------------------------------------------------------------------------
// Padding / margin
// ---------------------------------------------------------------------------

/// One value per edge, in terminal cells (top, right, bottom, left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BoxEdges {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl BoxEdges {
    pub const fn uniform(v: u16) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }

    pub const fn zero() -> Self {
        Self { top: 0, right: 0, bottom: 0, left: 0 }
    }

    /// Parse a CSS shorthand: `1`, `1 2`, `1 2 3`, or `1 2 3 4`.
    pub fn parse(shorthand: &str) -> Result<Self> {
        let parts: Vec<&str> = shorthand.split_whitespace().collect();
        let nums: Vec<u16> = parts
            .iter()
            .map(|p| {
                p.trim_end_matches("px").parse::<u16>().map_err(|_| CssError::InvalidLength(shorthand.into()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(match nums.len() {
            0 => Self::zero(),
            1 => Self::uniform(nums[0]),
            2 => Self { top: nums[0], bottom: nums[0], left: nums[1], right: nums[1] },
            3 => Self { top: nums[0], left: nums[1], right: nums[1], bottom: nums[2] },
            n => Self {
                top: nums[0],
                right: nums[1],
                bottom: nums[2 % n],
                left: nums[3 % n],
            },
        })
    }

    /// Project onto a ratatui `Padding` (used for `Block::padding`).
    pub fn to_padding(self) -> Padding {
        Padding::new(self.left, self.right, self.top, self.bottom)
    }

    /// Shrink a `Rect` outward by these edges (for `margin`).
    pub fn shrink(self, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
        let x = area.x.saturating_add(self.left);
        let y = area.y.saturating_add(self.top);
        let width = area
            .width
            .saturating_sub(self.left.saturating_add(self.right));
        let height = area
            .height
            .saturating_sub(self.top.saturating_add(self.bottom));
        ratatui::layout::Rect::new(x, y, width, height)
    }
}

// ---------------------------------------------------------------------------
// Border
// ---------------------------------------------------------------------------

/// Border drawing style. Width is implicit in the terminal (always 1 cell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    /// No border.
    #[default]
    None,
    /// A plain single-line border.
    Single,
    /// A rounded single-line border (`border-radius`).
    Rounded,
    /// A double-line border.
    Double,
    /// A thick single-line border.
    Thick,
}

impl BorderStyle {
    pub fn to_border_type(self) -> Option<BorderType> {
        match self {
            Self::None => None,
            Self::Single => Some(BorderType::Plain),
            Self::Rounded => Some(BorderType::Rounded),
            Self::Double => Some(BorderType::Double),
            Self::Thick => Some(BorderType::Thick),
        }
    }
}

/// A full border declaration: style + optional color.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderSpec {
    pub style: BorderStyle,
    pub color: Option<Color>,
}

impl Default for BorderSpec {
    fn default() -> Self {
        Self { style: BorderStyle::None, color: None }
    }
}

impl BorderSpec {
    /// The ratatui `Borders` set (all or none — per-edge is a P3 concern).
    pub fn borders(&self) -> Borders {
        match self.style {
            BorderStyle::None => Borders::NONE,
            _ => Borders::ALL,
        }
    }

    pub fn border_type(&self) -> BorderType {
        self.style.to_border_type().unwrap_or(BorderType::Plain)
    }

    /// Parse a CSS shorthand: `none` / `single` / `rounded` / `double` / `thick`,
    /// optionally with a width (`1px`) and a color (`rounded #f00`).
    pub fn parse_shorthand(s: &str) -> Result<Self> {
        let mut style = BorderStyle::None;
        let mut color_tokens: Vec<&str> = Vec::new();
        for tok in s.split_whitespace() {
            if tok.ends_with("px") {
                // width — present-but-ignored (terminal borders are always 1 cell).
                continue;
            }
            if let Some(parsed) = BorderStyle::parse_keyword(tok) {
                style = parsed;
            } else {
                color_tokens.push(tok);
            }
        }
        let color = if color_tokens.is_empty() {
            None
        } else {
            Some(Color::parse(&color_tokens.join(" "))?)
        };
        Ok(Self { style, color })
    }
}

impl BorderStyle {
    /// Parse a single keyword, case-insensitive.
    pub fn parse_keyword(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "none" | "hidden" => Self::None,
            "single" | "solid" | "plain" => Self::Single,
            "rounded" => Self::Rounded,
            "double" => Self::Double,
            "thick" => Self::Thick,
            _ => return None,
        })
    }

    pub fn as_keyword(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Single => "single",
            Self::Rounded => "rounded",
            Self::Double => "double",
            Self::Thick => "thick",
        }
    }
}

// ---------------------------------------------------------------------------
// Length / sizing
// ---------------------------------------------------------------------------

/// A one-dimensional size, mapped to a ratatui `Constraint`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// `auto` — let the layout engine decide (becomes `Min(0)`).
    Auto,
    /// A fixed cell count (`10`, `10px`).
    Cells(u16),
    /// A percentage of the available space (`50%`).
    Percent(u16),
    /// `min(n)` — at least `n` cells, grow if room.
    Min(u16),
    /// `max(n)` — at most `n` cells.
    Max(u16),
}

impl Length {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("auto") || s.is_empty() {
            return Ok(Self::Auto);
        }
        if let Some(rest) = s.strip_prefix("min(").and_then(|r| r.strip_suffix(')')) {
            return Ok(Self::Min(parse_cells(rest)?));
        }
        if let Some(rest) = s.strip_prefix("max(").and_then(|r| r.strip_suffix(')')) {
            return Ok(Self::Max(parse_cells(rest)?));
        }
        if let Some(rest) = s.strip_suffix('%') {
            return Ok(Self::Percent(rest.parse().map_err(|_| CssError::InvalidLength(s.into()))?));
        }
        Ok(Self::Cells(parse_cells(s)?))
    }

    pub fn to_constraint(self) -> Constraint {
        match self {
            Self::Auto => Constraint::Min(0),
            Self::Cells(n) => Constraint::Length(n),
            Self::Percent(p) => Constraint::Percentage(p),
            Self::Min(n) => Constraint::Min(n),
            Self::Max(n) => Constraint::Max(n),
        }
    }
}

fn parse_cells(s: &str) -> Result<u16> {
    s.trim_end_matches("px")
        .trim()
        .parse::<u16>()
        .map_err(|_| CssError::InvalidLength(s.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_shorthand() {
        assert_eq!(BoxEdges::parse("1").unwrap(), BoxEdges::uniform(1));
        let e = BoxEdges::parse("1 2").unwrap();
        assert_eq!((e.top, e.right, e.bottom, e.left), (1, 2, 1, 2));
        let e = BoxEdges::parse("1 2 3 4").unwrap();
        assert_eq!((e.top, e.right, e.bottom, e.left), (1, 2, 3, 4));
    }

    #[test]
    fn edges_shrink() {
        let area = ratatui::layout::Rect::new(0, 0, 10, 10);
        let inner = BoxEdges::uniform(1).shrink(area);
        assert_eq!((inner.x, inner.y, inner.width, inner.height), (1, 1, 8, 8));
    }

    #[test]
    fn length_parse() {
        assert_eq!(Length::parse("auto").unwrap(), Length::Auto);
        assert_eq!(Length::parse("10px").unwrap(), Length::Cells(10));
        assert_eq!(Length::parse("50%").unwrap(), Length::Percent(50));
        assert_eq!(Length::parse("min(3)").unwrap(), Length::Min(3));
    }
}

// ---------------------------------------------------------------------------
// Optional serde
// ---------------------------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_impl {
    use super::{BorderStyle, BorderSpec, BoxEdges, Length};
    use crate::color::Color;
    use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::Value;

    impl<'de> Deserialize<'de> for BoxEdges {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            match Value::deserialize(d)? {
                Value::Number(n) => {
                    let v = n.as_u64().unwrap_or(0) as u16;
                    Ok(BoxEdges::uniform(v))
                }
                Value::String(s) => BoxEdges::parse(&s).map_err(D::Error::custom),
                other => Err(D::Error::custom(format!("invalid padding/margin: {other}"))),
            }
        }
    }
    impl Serialize for BoxEdges {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if self.top == self.right && self.right == self.bottom && self.bottom == self.left {
                s.serialize_u64(self.top as u64)
            } else {
                s.serialize_str(&format!(
                    "{} {} {} {}",
                    self.top, self.right, self.bottom, self.left
                ))
            }
        }
    }

    impl<'de> Deserialize<'de> for Length {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            match Value::deserialize(d)? {
                Value::Number(n) => Ok(Length::Cells(n.as_u64().unwrap_or(0) as u16)),
                Value::String(s) => Length::parse(&s).map_err(D::Error::custom),
                other => Err(D::Error::custom(format!("invalid length: {other}"))),
            }
        }
    }
    impl Serialize for Length {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            match self {
                Length::Auto => s.serialize_str("auto"),
                Length::Cells(n) => s.serialize_str(&format!("{n}px")),
                Length::Percent(p) => s.serialize_str(&format!("{p}%")),
                Length::Min(n) => s.serialize_str(&format!("min({n})")),
                Length::Max(n) => s.serialize_str(&format!("max({n})")),
            }
        }
    }

    impl<'de> Deserialize<'de> for BorderStyle {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let s = String::deserialize(d)?;
            BorderStyle::parse_keyword(&s)
                .ok_or_else(|| D::Error::custom(format!("invalid border style: {s}")))
        }
    }
    impl Serialize for BorderStyle {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(self.as_keyword())
        }
    }

    impl<'de> Deserialize<'de> for BorderSpec {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            match Value::deserialize(d)? {
                Value::String(s) => BorderSpec::parse_shorthand(&s).map_err(D::Error::custom),
                Value::Object(map) => {
                    let style = match map.get("style") {
                        Some(v) => serde_json::from_value::<BorderStyle>(v.clone())
                            .map_err(D::Error::custom)?,
                        None => BorderStyle::None,
                    };
                    let color = match map.get("color") {
                        Some(Value::Null) | None => None,
                        Some(v) => Some(serde_json::from_value::<Color>(v.clone()).map_err(D::Error::custom)?),
                    };
                    Ok(BorderSpec { style, color })
                }
                other => Err(D::Error::custom(format!("invalid border: {other}"))),
            }
        }
    }
    impl Serialize for BorderSpec {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeStruct;
            let mut st = s.serialize_struct("BorderSpec", 2)?;
            st.serialize_field("style", &self.style)?;
            st.serialize_field("color", &self.color)?;
            st.end()
        }
    }
}
