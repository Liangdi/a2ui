//! Bridge from [`ratatui-themekit`] semantic color slots to CSS custom properties.
//!
//! Gated behind the `themekit` feature. Lets a stylesheet write
//! `color: var(--accent)` and have it resolve through themekit's palettes
//! unchanged.
//!
//! [`ratatui-themekit`]: https://docs.rs/ratatui-themekit

use ratatui_themekit::Theme as ThemekitTheme;

use crate::color::Color;
use crate::token::ThemeTokens;

impl ThemeTokens {
    /// Seed CSS custom properties from a themekit theme.
    ///
    /// Maps all 15 semantic slots (plus `background`) to `--name` variables:
    /// `--accent`, `--accent-dim`, `--text`, `--text-dim`, `--text-bright`,
    /// `--success`, `--error`, `--warning`, `--info`, `--diff-added`,
    /// `--diff-removed`, `--diff-context`, `--border`, `--surface`,
    /// `--background`.
    pub fn from_themekit<T: ThemekitTheme>(theme: &T) -> ThemeTokens {
        let mut t = ThemeTokens::new();
        t.insert("accent", Color::Literal(theme.accent()));
        t.insert("accent-dim", Color::Literal(theme.accent_dim()));
        t.insert("text", Color::Literal(theme.text()));
        t.insert("text-dim", Color::Literal(theme.text_dim()));
        t.insert("text-bright", Color::Literal(theme.text_bright()));
        t.insert("success", Color::Literal(theme.success()));
        t.insert("error", Color::Literal(theme.error()));
        t.insert("warning", Color::Literal(theme.warning()));
        t.insert("info", Color::Literal(theme.info()));
        t.insert("diff-added", Color::Literal(theme.diff_added()));
        t.insert("diff-removed", Color::Literal(theme.diff_removed()));
        t.insert("diff-context", Color::Literal(theme.diff_context()));
        t.insert("border", Color::Literal(theme.border()));
        t.insert("surface", Color::Literal(theme.surface()));
        t.insert("background", Color::Literal(theme.background()));
        t
    }
}
