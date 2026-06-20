//! Tabs component — renders a horizontal tab bar with content area.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::component_impl::TuiComponent;
use a2ui_base::event::{EventResult, InputEvent, InputKey};
use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::{DynamicNumber, DynamicString};

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
/// The active tab index is read from the `activeTab` property (a `DynamicNumber`).
/// Arrow keys cycle through tabs and write the new index back via `EventResult::DataUpdate`.
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
        render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
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

        // Resolve active tab index from the `activeTab` property.
        let active_tab: usize = comp_model
            .get_property::<DynamicNumber>("activeTab")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn) as usize)
            .unwrap_or(0)
            .min(tabs.len() - 1);

        // Split area: 3 rows for tab bar, rest for content.
        let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);

        let tab_bar_area = chunks[0];
        let content_area = chunks[1];

        // Build tab title spans.
        let spans: Vec<Span> = tabs
            .iter()
            .enumerate()
            .flat_map(|(i, tab)| {
                let title = ctx.data_context.resolve_dynamic_string(&tab.title);
                let style = if i == active_tab {
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

        // Render the active tab's child.
        if content_area.width > 0 && content_area.height > 0 {
            render_child(&tabs[active_tab].child, content_area, frame, "");
        }
    }

    fn natural_height(
        &self,
        ctx: &ComponentContext,
        available_width: u16,
        measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        let comp_model = ctx.components.get(&ctx.component_id)?;
        let tabs: Vec<TabEntry> = comp_model.get_property("tabs")?;
        if tabs.is_empty() {
            return None;
        }

        // Resolve active tab index from the `activeTab` property.
        let active_tab: usize = comp_model
            .get_property::<DynamicNumber>("activeTab")
            .map(|dn| ctx.data_context.resolve_dynamic_number(&dn) as usize)
            .unwrap_or(0)
            .min(tabs.len() - 1);

        let child_h = measure_child(&tabs[active_tab].child, "", available_width)?;
        Some(child_h.saturating_add(3))
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &a2ui_base::event::InputEvent,
    ) -> Option<a2ui_base::event::EventResult> {
        let comp_model = ctx.components.get(&ctx.component_id)?;
        let tabs: Vec<TabEntry> = comp_model.get_property("tabs")?;
        if tabs.is_empty() {
            return None;
        }

        let active_tab_dn = comp_model.get_property::<DynamicNumber>("activeTab")?;
        let binding = match &active_tab_dn {
            DynamicNumber::Binding(b) => b.clone(),
            _ => return None,
        };

        let current = ctx.data_context.resolve_dynamic_number(&active_tab_dn) as usize;
        let current = current.min(tabs.len() - 1);

        let new_idx = match event {
            InputEvent::KeyPress {
                key: InputKey::Right,
            } => (current + 1) % tabs.len(),
            InputEvent::KeyPress {
                key: InputKey::Left,
            } => {
                if current == 0 {
                    tabs.len() - 1
                } else {
                    current - 1
                }
            }
            _ => return None,
        };

        Some(EventResult::DataUpdate {
            path: binding.path.clone(),
            value: serde_json::json!(new_idx),
        })
    }
}
