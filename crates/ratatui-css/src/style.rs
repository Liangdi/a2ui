//! `CssStyle` — a CSS declaration block projected onto ratatui primitives.
//!
//! Every field is optional; `None` means "not declared", which is the cascade's
//! "do not override" signal (and mirrors ratatui's own `Option`-based `Style`).
//!
//! Projections:
//! - decoration fields → [`ratatui::style::Style`] via [`CssStyle::to_style`]
//! - box-model fields  → [`ratatui::widgets::Block`] / `Rect` shrink
//! - sizing fields     → [`ratatui::layout::Constraint`] / [`ratatui::layout::Alignment`]

use ratatui::{
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style as RStyle},
    widgets::Block,
};

use crate::box_model::{BorderSpec, BoxEdges, Length};
use crate::color::Color;

// ---------------------------------------------------------------------------
// Property enums
// ---------------------------------------------------------------------------

/// `font-weight`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weight {
    #[default]
    Normal,
    Bold,
}

/// `font-style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

/// `text-decoration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    LineThrough,
    UnderlineLineThrough,
}

impl TextDecoration {
    fn modifiers(self) -> Option<Modifier> {
        match self {
            Self::None => None,
            Self::Underline => Some(Modifier::UNDERLINED),
            Self::LineThrough => Some(Modifier::CROSSED_OUT),
            Self::UnderlineLineThrough => Some(Modifier::UNDERLINED.union(Modifier::CROSSED_OUT)),
        }
    }
}

/// `text-align`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    pub fn to_alignment(self) -> Alignment {
        match self {
            Self::Left => Alignment::Left,
            Self::Center => Alignment::Center,
            Self::Right => Alignment::Right,
        }
    }
}

// ---------------------------------------------------------------------------
// CssStyle
// ---------------------------------------------------------------------------

/// A CSS declaration block.
///
/// Construct via builder, deserialize (with the `serde` feature), or receive as
/// a [`crate::cascade::ComputedStyle`] after cascade resolution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CssStyle {
    // — decoration → Style —
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub weight: Option<Weight>,
    pub font_style: Option<FontStyle>,
    pub decoration: Option<TextDecoration>,
    pub underline_color: Option<Color>,

    // — box model → Block / Rect —
    pub padding: Option<BoxEdges>,
    pub margin: Option<BoxEdges>,
    pub border: Option<BorderSpec>,

    // — sizing → Constraint / Alignment —
    pub text_align: Option<Align>,
    pub width: Option<Length>,
    pub height: Option<Length>,
}

impl CssStyle {
    /// An empty declaration block (no declarations).
    pub fn new() -> Self {
        Self::default()
    }

    // --- builders ----------------------------------------------------------

    pub fn color(mut self, c: impl Into<Color>) -> Self {
        self.color = Some(c.into());
        self
    }
    pub fn background(mut self, c: impl Into<Color>) -> Self {
        self.background = Some(c.into());
        self
    }
    pub fn bold(mut self) -> Self {
        self.weight = Some(Weight::Bold);
        self
    }
    pub fn italic(mut self) -> Self {
        self.font_style = Some(FontStyle::Italic);
        self
    }
    pub fn underline(mut self) -> Self {
        self.decoration = Some(TextDecoration::Underline);
        self
    }

    /// Box-model builders. These parse CSS shorthand strings and panic on a
    /// malformed value — intended for builder ergonomics where the literal is
    /// known at compile time. For data-driven input, deserialize instead.
    pub fn padding(mut self, shorthand: &str) -> Self {
        self.padding = Some(crate::box_model::BoxEdges::parse(shorthand).expect("valid padding"));
        self
    }
    pub fn margin(mut self, shorthand: &str) -> Self {
        self.margin = Some(crate::box_model::BoxEdges::parse(shorthand).expect("valid margin"));
        self
    }
    pub fn border(mut self, shorthand: &str) -> Self {
        self.border = Some(crate::box_model::BorderSpec::parse_shorthand(shorthand).expect("valid border"));
        self
    }

    // --- cascade -----------------------------------------------------------

    /// Overlay `other` onto `self`: every field that is `Some` in `other`
    /// replaces the corresponding field in `self`. `None` fields are left
    /// untouched. This is the per-declaration step of the cascade.
    pub fn overlay(&mut self, other: &CssStyle) {
        macro_rules! over {
            ($f:ident) => {
                if other.$f.is_some() {
                    self.$f = other.$f.clone();
                }
            };
        }
        over!(color);
        over!(background);
        over!(weight);
        over!(font_style);
        over!(decoration);
        over!(underline_color);
        over!(padding);
        over!(margin);
        over!(border);
        over!(text_align);
        over!(width);
        over!(height);
    }

    /// Fill inheritable fields that are still `None` from `parent`.
    /// Inheritable: `color`, `weight`, `font_style`, `decoration`,
    /// `underline_color`, `text_align`.
    pub fn inherit_from(&mut self, parent: &CssStyle) {
        if self.color.is_none() {
            self.color = parent.color.clone();
        }
        if self.weight.is_none() {
            self.weight = parent.weight;
        }
        if self.font_style.is_none() {
            self.font_style = parent.font_style;
        }
        if self.decoration.is_none() {
            self.decoration = parent.decoration;
        }
        if self.underline_color.is_none() {
            self.underline_color = parent.underline_color.clone();
        }
        if self.text_align.is_none() {
            self.text_align = parent.text_align;
        }
    }

    // --- projections -------------------------------------------------------

    /// Resolve a [`Color`] for direct application: `Literal`/`Reset` map to a
    /// ratatui color, `Var`/`Inherit` map to `None` (caller leaves it unset).
    /// After cascade resolution no `Var`/`Inherit` should remain.
    fn paint(c: &Color) -> Option<ratatui::style::Color> {
        match c {
            Color::Literal(lc) => Some(*lc),
            Color::Reset => Some(ratatui::style::Color::Reset),
            Color::Var { .. } | Color::Inherit => None,
        }
    }

    /// Build a ratatui `Style` from the decoration fields.
    pub fn to_style(&self) -> RStyle {
        let mut s = RStyle::default();
        if let Some(c) = self.color.as_ref().and_then(Self::paint) {
            s = s.fg(c);
        }
        if let Some(c) = self.background.as_ref().and_then(Self::paint) {
            s = s.bg(c);
        }
        if let Some(c) = self.underline_color.as_ref().and_then(Self::paint) {
            s = s.underline_color(c);
        }
        if self.weight == Some(Weight::Bold) {
            s = s.add_modifier(Modifier::BOLD);
        }
        if self.font_style == Some(FontStyle::Italic) {
            s = s.add_modifier(Modifier::ITALIC);
        }
        if let Some(m) = self.decoration.and_then(|d| d.modifiers()) {
            s = s.add_modifier(m);
        }
        s
    }

    /// Build a ratatui `Block` from the box-model fields.
    ///
    /// `border` sets borders/type/border-color; `padding` sets inner padding;
    /// `background` sets the block's fill style.
    pub fn to_block(&self) -> Block<'_> {
        let mut block = Block::default();
        if let Some(b) = &self.border {
            block = block.borders(b.borders()).border_type(b.border_type());
            if let Some(c) = b.color.as_ref().and_then(Self::paint) {
                block = block.border_style(RStyle::default().fg(c));
            }
        }
        if let Some(pad) = self.padding {
            block = block.padding(pad.to_padding());
        }
        if let Some(c) = self.background.as_ref().and_then(Self::paint) {
            block = block.style(RStyle::default().bg(c));
        }
        block
    }

    /// Shrink `area` by the `margin` edges, if any.
    pub fn apply_margin(&self, area: Rect) -> Rect {
        match self.margin {
            Some(e) => e.shrink(area),
            None => area,
        }
    }

    /// `(width, height)` constraints, if either is declared.
    pub fn constraints(&self) -> Option<(Constraint, Constraint)> {
        if self.width.is_none() && self.height.is_none() {
            return None;
        }
        let w = self.width.map(|l| l.to_constraint()).unwrap_or(Constraint::Min(0));
        let h = self.height.map(|l| l.to_constraint()).unwrap_or(Constraint::Min(0));
        Some((w, h))
    }

    /// The resolved text alignment, if declared.
    pub fn alignment(&self) -> Option<Alignment> {
        self.text_align.map(|a| a.to_alignment())
    }

    /// `true` if no declarations are set.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

// Convenience: literals coerce into `Color`.
impl From<ratatui::style::Color> for Color {
    fn from(c: ratatui::style::Color) -> Self {
        Color::Literal(c)
    }
}

// ---------------------------------------------------------------------------
// Optional serde — deserialize a CssStyle from a property map keyed by CSS
// property names; serialize back to the same shape.
// ---------------------------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_impl {
    use super::{Align, CssStyle, FontStyle, TextDecoration, Weight};
    use serde::de::Error as DeError;
    use serde::ser::Error as SerError;
    use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::{Map, Value};

    // --- keyword enums ---

    impl<'de> Deserialize<'de> for Align {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let s = String::deserialize(d)?.to_ascii_lowercase();
            match s.as_str() {
                "left" | "justify" => Ok(Align::Left),
                "center" => Ok(Align::Center),
                "right" => Ok(Align::Right),
                _ => Err(DeError::custom(format!("invalid text-align: {s}"))),
            }
        }
    }
    impl Serialize for Align {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(match self {
                Align::Left => "left",
                Align::Center => "center",
                Align::Right => "right",
            })
        }
    }

    impl<'de> Deserialize<'de> for FontStyle {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            match String::deserialize(d)?.to_ascii_lowercase().as_str() {
                "normal" => Ok(FontStyle::Normal),
                "italic" | "oblique" => Ok(FontStyle::Italic),
                other => Err(DeError::custom(format!("invalid font-style: {other}"))),
            }
        }
    }
    impl Serialize for FontStyle {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(match self {
                FontStyle::Normal => "normal",
                FontStyle::Italic => "italic",
            })
        }
    }

    impl<'de> Deserialize<'de> for Weight {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            match Value::deserialize(d)? {
                Value::String(s) => match s.to_ascii_lowercase().as_str() {
                    "bold" | "bolder" => Ok(Weight::Bold),
                    "normal" | "lighter" => Ok(Weight::Normal),
                    other => other
                        .parse::<u32>()
                        .map(|n| if n >= 600 { Weight::Bold } else { Weight::Normal })
                        .map_err(|_| DeError::custom(format!("invalid font-weight: {s}"))),
                },
                Value::Number(n) => Ok(if n.as_i64().map(|i| i >= 600).unwrap_or(false) {
                    Weight::Bold
                } else {
                    Weight::Normal
                }),
                other => Err(DeError::custom(format!("invalid font-weight: {other}"))),
            }
        }
    }
    impl Serialize for Weight {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(match self {
                Weight::Normal => "normal",
                Weight::Bold => "bold",
            })
        }
    }

    impl<'de> Deserialize<'de> for TextDecoration {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let s = String::deserialize(d)?;
            let lower = s.to_ascii_lowercase();
            let has_u = lower.split_whitespace().any(|t| t == "underline");
            let has_l =
                lower.split_whitespace().any(|t| t == "line-through" || t == "strikethrough");
            Ok(match (has_u, has_l) {
                (false, false) => TextDecoration::None,
                (true, false) => TextDecoration::Underline,
                (false, true) => TextDecoration::LineThrough,
                (true, true) => TextDecoration::UnderlineLineThrough,
            })
        }
    }
    impl Serialize for TextDecoration {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(match self {
                TextDecoration::None => "none",
                TextDecoration::Underline => "underline",
                TextDecoration::LineThrough => "line-through",
                TextDecoration::UnderlineLineThrough => "underline line-through",
            })
        }
    }

    // --- CssStyle as a property map ---

    fn parse_opt<T: DeserializeOwned>(v: Value) -> Result<Option<T>, serde_json::Error> {
        if v.is_null() {
            return Ok(None);
        }
        serde_json::from_value(v).map(Some)
    }

    impl<'de> Deserialize<'de> for CssStyle {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let map = Map::<String, Value>::deserialize(d)?;
            let mut s = CssStyle::default();
            for (key, val) in map {
                let res: Result<(), &str> = match key.to_ascii_lowercase().as_str() {
                    "color" => parse_opt(val).map(|v| s.color = v).map_err(|_| "color"),
                    "background" | "background-color" => {
                        parse_opt(val).map(|v| s.background = v).map_err(|_| "background")
                    }
                    "font-weight" => parse_opt(val).map(|v| s.weight = v).map_err(|_| "font-weight"),
                    "font-style" => parse_opt(val).map(|v| s.font_style = v).map_err(|_| "font-style"),
                    "text-decoration" => {
                        parse_opt(val).map(|v| s.decoration = v).map_err(|_| "text-decoration")
                    }
                    "underline-color" => {
                        parse_opt(val).map(|v| s.underline_color = v).map_err(|_| "underline-color")
                    }
                    "padding" => parse_opt(val).map(|v| s.padding = v).map_err(|_| "padding"),
                    "margin" => parse_opt(val).map(|v| s.margin = v).map_err(|_| "margin"),
                    "border" => parse_opt(val).map(|v| s.border = v).map_err(|_| "border"),
                    "text-align" => parse_opt(val).map(|v| s.text_align = v).map_err(|_| "text-align"),
                    "width" => parse_opt(val).map(|v| s.width = v).map_err(|_| "width"),
                    "height" => parse_opt(val).map(|v| s.height = v).map_err(|_| "height"),
                    _ => continue, // unknown property → ignored (forward-compat)
                };
                res.map_err(|name| DeError::custom(format!("invalid {name} value")))?;
            }
            Ok(s)
        }
    }

    impl Serialize for CssStyle {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut map = Map::new();
            macro_rules! put {
                ($key:expr, $field:expr) => {
                    if let Some(v) = $field {
                        map.insert($key.into(), serde_json::to_value(v).map_err(SerError::custom)?);
                    }
                };
            }
            put!("color", self.color.as_ref());
            put!("background-color", self.background.as_ref());
            put!("font-weight", self.weight);
            put!("font-style", self.font_style);
            put!("text-decoration", self.decoration);
            put!("underline-color", self.underline_color.as_ref());
            put!("padding", self.padding);
            put!("margin", self.margin);
            put!("border", self.border.as_ref());
            put!("text-align", self.text_align);
            put!("width", self.width);
            put!("height", self.height);
            map.serialize(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color as RC;

    #[test]
    fn to_style_maps_decoration() {
        let s = CssStyle::new().color(RC::Red).background(RC::Blue).bold().italic();
        let rs = s.to_style();
        assert_eq!(rs.fg, Some(RC::Red));
        assert_eq!(rs.bg, Some(RC::Blue));
        assert!(rs.add_modifier.contains(Modifier::BOLD));
        assert!(rs.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn overlay_only_sets_some() {
        let mut a = CssStyle::new().color(RC::Red).bold();
        let b = CssStyle::new().color(RC::Blue); // higher priority: color only
        a.overlay(&b);
        assert_eq!(a.color, Some(Color::literal(RC::Blue)));
        assert_eq!(a.weight, Some(Weight::Bold)); // survives
    }

    #[test]
    fn inherit_from_parent() {
        let parent = CssStyle::new().color(RC::Green).italic();
        let mut child = CssStyle::new().bold(); // no color
        child.inherit_from(&parent);
        assert_eq!(child.color, Some(Color::literal(RC::Green)));
        assert_eq!(child.weight, Some(Weight::Bold)); // own
        assert_eq!(child.font_style, Some(FontStyle::Italic)); // inherited
    }

    #[test]
    fn apply_margin_and_block() {
        let s = CssStyle::new();
        let area = Rect::new(0, 0, 10, 10);
        assert_eq!(s.apply_margin(area), area); // no margin
    }
}
