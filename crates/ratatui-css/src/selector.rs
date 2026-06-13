//! CSS selectors — the pragmatic subset: compound selectors of the form
//! `Type.class#id:pseudo…` (plus comma lists and the `*` universal).
//!
//! Descendant/child/sibling combinators (`A B`, `A > B`, `A + B`) are P3.

use crate::error::{CssError, Result};
use crate::node::StyledNode;

/// A single pseudo-class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoClass {
    Focus,
    Hover,
    Disabled,
    Checked,
    Active,
}

impl PseudoClass {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "focus" => Self::Focus,
            "hover" => Self::Hover,
            "disabled" => Self::Disabled,
            "checked" => Self::Checked,
            "active" => Self::Active,
            _ => return None,
        })
    }
}

/// A compound selector: an optional type, plus class/id/pseudo qualifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub type_name: Option<String>,
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub pseudos: Vec<PseudoClass>,
}

impl Default for Selector {
    fn default() -> Self {
        Self::universal()
    }
}

impl Selector {
    /// The universal selector `*` — matches every element.
    pub fn universal() -> Self {
        Self { type_name: None, classes: Vec::new(), id: None, pseudos: Vec::new() }
    }

    /// Parse one or more comma-separated selectors.
    pub fn parse_list(s: &str) -> Result<Vec<Self>> {
        s.split(',')
            .map(|part| Self::parse_compound(part.trim()))
            .collect()
    }

    /// Parse a single compound selector.
    pub fn parse_compound(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(CssError::InvalidSelector("empty selector".into()));
        }

        let mut sel = Self::universal();
        let mut chars = s.char_indices().peekable();
        let len = s.len();

        // Optional leading type name or `*`.
        if let Some(&(_, c)) = chars.peek() {
            if c == '*' {
                chars.next();
            } else if !matches!(c, '.' | '#' | ':') {
                let start = 0usize;
                let mut end = 0usize;
                while let Some(&(i, c)) = chars.peek() {
                    if matches!(c, '.' | '#' | ':') {
                        break;
                    }
                    end = i + c.len_utf8();
                    chars.next();
                }
                sel.type_name = Some(s[start..end].to_string());
            }
        }

        while let Some(&(i, c)) = chars.peek() {
            chars.next(); // consume delimiter
            let start = i + c.len_utf8();
            let mut end = start;
            while let Some(&(j, ch)) = chars.peek() {
                if matches!(ch, '.' | '#' | ':') {
                    break;
                }
                end = j + ch.len_utf8();
                chars.next();
            }
            if end == start {
                return Err(CssError::InvalidSelector(format!(
                    "selector `{s}` has a dangling `{c}`"
                )));
            }
            let token = &s[start..end];
            match c {
                '.' => sel.classes.push(token.to_string()),
                '#' => {
                    if sel.id.is_some() {
                        return Err(CssError::InvalidSelector(format!(
                            "selector `{s}` has multiple ids"
                        )));
                    }
                    sel.id = Some(token.to_string());
                }
                ':' => match PseudoClass::parse(token) {
                    Some(p) => sel.pseudos.push(p),
                    None => {
                        return Err(CssError::InvalidSelector(format!(
                            "unsupported pseudo-class `:{token}`"
                        )))
                    }
                },
                _ => unreachable!("delimiter handled above"),
            }
        }

        let _ = len;
        Ok(sel)
    }

    /// Specificity as `(ids, classes_and_pseudos, type)`, comparable as a tuple.
    pub fn specificity(&self) -> (u32, u32, u32) {
        let ids = if self.id.is_some() { 1 } else { 0 };
        let cp = (self.classes.len() + self.pseudos.len()) as u32;
        let ty = if self.type_name.is_some() { 1 } else { 0 };
        (ids, cp, ty)
    }

    /// Whether this selector matches a given node (including pseudo-state).
    pub fn matches(&self, node: &dyn StyledNode) -> bool {
        // Type: case-insensitive (convenience); universal matches anything.
        if let Some(t) = &self.type_name
            && !node.type_name().eq_ignore_ascii_case(t)
        {
            return false;
        }
        // Id: case-sensitive.
        if let Some(id) = &self.id
            && node.id() != Some(id.as_str())
        {
            return false;
        }
        // Classes: all must be present (case-sensitive).
        let node_classes = node.classes();
        for c in &self.classes {
            if !node_classes.contains(&c.as_str()) {
                return false;
            }
        }
        // Pseudo-classes: all must be reflected in the node's state.
        let st = node.state();
        for p in &self.pseudos {
            let on = match p {
                PseudoClass::Focus => st.focus,
                PseudoClass::Hover => st.hover,
                PseudoClass::Disabled => st.disabled,
                PseudoClass::Checked => st.checked,
                PseudoClass::Active => st.active,
            };
            if !on {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{OwnedNode, State};

    #[test]
    fn parse_compound() {
        let s = Selector::parse_compound("Button.primary#save:focus").unwrap();
        assert_eq!(s.type_name.as_deref(), Some("Button"));
        assert_eq!(s.classes, vec!["primary"]);
        assert_eq!(s.id.as_deref(), Some("save"));
        assert_eq!(s.pseudos, vec![PseudoClass::Focus]);
        assert_eq!(s.specificity(), (1, 2, 1));
    }

    #[test]
    fn universal_specificity() {
        assert_eq!(Selector::universal().specificity(), (0, 0, 0));
    }

    #[test]
    fn matching() {
        let sel = Selector::parse_compound("Button.primary").unwrap();
        let n = OwnedNode::new("Button").with_classes(["primary"]);
        assert!(sel.matches(&n));

        let wrong_type = OwnedNode::new("Text").with_classes(["primary"]);
        assert!(!sel.matches(&wrong_type));

        let missing_class = OwnedNode::new("Button");
        assert!(!sel.matches(&missing_class));
    }

    #[test]
    fn matching_with_state() {
        let sel = Selector::parse_compound("Button:disabled").unwrap();
        let on = OwnedNode::new("Button").with_state(State::disabled());
        let off = OwnedNode::new("Button");
        assert!(sel.matches(&on));
        assert!(!sel.matches(&off));
    }

    #[test]
    fn comma_list() {
        let list = Selector::parse_list("Text, .muted, #title").unwrap();
        assert_eq!(list.len(), 3);
    }
}
