//! Tabs component — renders a horizontal tab bar with content area.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicString;
use crate::tui::component_impl::TuiComponent;

/// Tab entry deserialized from the `tabs` property.
#[derive(Debug, Clone, serde::Deserialize)]
struct TabEntry {
    title: DynamicString,
    child: String,
}

/// Tabs component implementation.
///
/// Renders a horizontal row of tab titles with the active tab highlighted,
/// and the active tab's child content below the tab bar.
///
/// Since `TuiComponent::render` is stateless, the active tab is always index 0.
pub struct TabsComponent;

impl TuiComponent for TabsComponent {
    fn name(&self) -> &'static str {
        "Tabs"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame),
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        let tabs: Vec<TabEntry> = match comp_model.get_property("tabs") {
            Some(t) => t,
            None => return,
        };

        if tabs.is_empty() {
            return;
        }

        // Split area: 3 rows for tab bar, rest for content.
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

        let tab_bar_area = chunks[0];
        let content_area = chunks[1];

        // Build tab title spans.
        let spans: Vec<Span> = tabs
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let title = ctx.data_context.resolve_dynamic_string(&tab.title);
                let style = if i == 0 {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let separator = if i < tabs.len() - 1 {
                    Span::raw(" | ")
                } else {
                    Span::raw("")
                };
                vec![Span::styled(format!(" {} ", title), style), separator]
            })
            .collect();

        // Render tab bar.
        let tab_bar = Paragraph::new(Line::from(spans));
        frame.render_widget(tab_bar, tab_bar_area);

        // Render the active tab's child (always index 0).
        if content_area.width > 0 && content_area.height > 0 {
            render_child(&tabs[0].child, content_area, frame);
        }
    }
}
