//! Framework-agnostic element view: the `StyledNode` trait and supporting
//! types. The cascade engine knows nothing about a2ui, ratatui widgets, or any
//! particular framework — it only knows a [`StyledNode`].

/// A set of pseudo-class flags for one element.
///
/// Maps directly to CSS pseudo-classes: `:focus`, `:hover`, `:disabled`,
/// `:checked`, `:active`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    pub focus: bool,
    pub hover: bool,
    pub disabled: bool,
    pub checked: bool,
    pub active: bool,
}

impl State {
    pub const fn empty() -> Self {
        Self { focus: false, hover: false, disabled: false, checked: false, active: false }
    }
    pub const fn focus() -> Self {
        Self { focus: true, ..Self::empty() }
    }
    pub const fn disabled() -> Self {
        Self { disabled: true, ..Self::empty() }
    }
}

/// Where an element sits among its siblings. Used by future `:nth-child`
/// matching (P3); returned by [`StyledNode::position`] for forward-compat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Position {
    /// 0-based index among siblings.
    pub index: usize,
    /// Total number of siblings (including self).
    pub sibling_count: usize,
    /// The parent element's type name, if any.
    pub parent_type: Option<String>,
}

impl Position {
    pub fn new(index: usize, sibling_count: usize) -> Self {
        Self { index, sibling_count, parent_type: None }
    }
}

/// The minimal contract the cascade needs to match selectors against an
/// element.
///
/// Implement this on your framework's node type (e.g. a2ui's `ComponentModel`,
/// or a plain app-state struct in a vanilla ratatui app).
pub trait StyledNode {
    /// Element type name — matches a CSS type selector (e.g. `"Button"`).
    fn type_name(&self) -> &str;

    /// Element id — matches a CSS `#id` selector.
    fn id(&self) -> Option<&str>;

    /// Class names — match CSS `.class` selectors.
    ///
    /// Returns a `Vec<&str>` (borrowing `self`) rather than a slice so that
    /// implementors storing `String`s don't need a secondary buffer. The
    /// allocation is acceptable for v1; the future `ComputedStyle` cache (P3)
    /// amortizes it.
    fn classes(&self) -> Vec<&str>;

    /// Pseudo-class state — matches `:focus` / `:disabled` / etc.
    fn state(&self) -> State;

    /// Sibling position — for future `:nth-child` support.
    fn position(&self) -> Position;
}

/// A trivial node for tests/examples — own its data.
#[derive(Debug, Clone)]
pub struct OwnedNode {
    pub type_name: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub state: State,
    pub position: Position,
}

impl OwnedNode {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            id: None,
            classes: Vec::new(),
            state: State::empty(),
            position: Position::default(),
        }
    }
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    pub fn with_classes(mut self, classes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.classes = classes.into_iter().map(Into::into).collect();
        self
    }
    pub fn with_state(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl StyledNode for OwnedNode {
    fn type_name(&self) -> &str {
        &self.type_name
    }
    fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    fn classes(&self) -> Vec<&str> {
        self.classes.iter().map(String::as_str).collect()
    }
    fn state(&self) -> State {
        self.state
    }
    fn position(&self) -> Position {
        self.position.clone()
    }
}
