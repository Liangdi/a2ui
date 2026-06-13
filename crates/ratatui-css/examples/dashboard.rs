//! Styled dashboard — a single-frame gallery of widget types, all restyled
//! from one CSS stylesheet. Demonstrates breadth: header, a styled list,
//! variant paragraphs (inheritance from the panel), a button row, and footer.
//!
//! ```sh
//! cargo run -p ratatui-css --example dashboard
//! ```

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Terminal, backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, List, ListItem, Paragraph},
};

use ratatui_css::{OwnedNode, State, Stylesheet};

type Term = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sheet = Stylesheet::parse(
        r##"
        :root {
            --accent: #89b4fa;
            --green: #a6e3a1;
            --peach: #fab387;
            --bg: #1e1e2e;
            --panel: #313244;
            --text: #cdd6f4;
            --muted: #6c7086;
        }

        Root        { background: var(--bg); }
        Header      { color: var(--accent); background: var(--panel); font-weight: bold; }
        Footer      { color: var(--muted); }

        /* Panels set a text color their children inherit. */
        Panel       { color: var(--text); border: rounded; padding: 1; }
        Panel.side  { border: rounded; }

        /* Variant text — class-based. */
        Text.title  { color: var(--accent); font-weight: bold; }
        Text.ok     { color: var(--green); }
        Text.warn   { color: var(--peach); }
        Text.muted  { color: var(--muted); }

        /* List rows. */
        List        { color: var(--text); }
        ListRow     { color: var(--muted); }
        ListRow.active { color: var(--accent); font-weight: bold; }

        Button          { color: var(--text); padding: 0 2; }
        Button.primary  { background: var(--accent); color: #1e1e2e; font-weight: bold; }
        Button.ghost    { border: rounded; }
        Button:disabled { color: var(--muted); }
        "##,
    )?;

    let mut terminal = setup()?;
    loop {
        terminal.draw(|f| draw(f, &sheet))?;
        if event::poll(Duration::from_millis(120))?
            && let Event::Key(key) = event::read()?
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            break;
        }
    }
    teardown(&mut terminal)?;
    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, sheet: &Stylesheet) {
    let area = frame.area();
    let root = sheet.compute(&OwnedNode::new("Root"), None);
    frame.render_widget(Block::default().style(root.to_style()), area);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
        .split(area);

    // Header.
    let header = sheet.compute(&OwnedNode::new("Header"), None);
    frame.render_widget(
        Paragraph::new(Line::from(" ◆ ratatui-css dashboard"))
            .style(header.to_style())
            .alignment(Alignment::Left),
        outer[0],
    );

    // Body: side list | main panel.
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(1)])
        .split(outer[1]);

    // Side panel with a styled list.
    let side = sheet.compute(&OwnedNode::new("Panel").with_classes(["side"]), None);
    let side_inner = render_block(frame, side.to_block(), body[0]);
    let rows = ["Inbox", "Drafts", "Sent", "Archive"];
    let items: Vec<ListItem<'_>> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let classes: Vec<&str> = if i == 0 { vec!["active"] } else { vec![] };
            let node = OwnedNode::new("ListRow")
                .with_classes(classes)
                .with_state(State::empty());
            let st = sheet.compute(&node, Some(&side)).to_style();
            ListItem::new(Line::from(format!(" {r}"))).style(st)
        })
        .collect();
    let list_style = sheet.compute(&OwnedNode::new("List"), Some(&side)).to_style();
    frame.render_widget(List::new(items).style(list_style), side_inner);

    // Main panel — children inherit the panel text color.
    let panel = sheet.compute(&OwnedNode::new("Panel"), None);
    let panel_inner = render_block(frame, panel.to_block(), body[1]);
    let panel_area = panel_inner;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1), Constraint::Length(3)])
        .split(panel_area);

    let title = sheet.compute(&OwnedNode::new("Text").with_classes(["title"]), Some(&panel));
    frame.render_widget(
        Paragraph::new(Line::from("Status")).style(title.to_style()),
        chunks[0],
    );

    // Variant paragraphs.
    let lines = [
        ("Service is healthy", "ok"),
        ("Queue depth above threshold", "warn"),
        ("Last sync 2 minutes ago", "muted"),
        ("(this line inherits the panel text color)", ""),
    ];
    let mut merged: Vec<Line<'_>> = Vec::new();
    for (text, cls) in lines {
        let node = OwnedNode::new("Text").with_classes([cls]);
        let st = sheet.compute(&node, Some(&panel)).to_style();
        merged.push(Line::from(format!(" • {text}")).style(st));
    }
    frame.render_widget(
        List::new(merged.into_iter().collect::<Vec<_>>()),
        chunks[1],
    );

    // Button row.
    let btn_rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(33), Constraint::Percentage(33)])
        .split(chunks[2]);
    let buttons = [
        ("Deploy", "primary", false),
        ("Cancel", "ghost", false),
        ("Remove", "ghost", true),
    ];
    for (rect, (label, class, dis)) in btn_rects.iter().zip(buttons.iter()) {
        let node = OwnedNode::new("Button")
            .with_classes([*class])
            .with_state(State { disabled: *dis, ..State::empty() });
        let computed = sheet.compute(&node, None);
        let para = Paragraph::new(Line::from(format!(" {label} ")))
            .style(computed.to_style())
            .alignment(Alignment::Center);
        let block = computed.to_block();
        frame.render_widget(para.block(block), *rect);
    }

    // Footer.
    let footer = sheet.compute(&OwnedNode::new("Footer"), None);
    frame.render_widget(
        Paragraph::new(Line::from(" q to quit")).style(footer.to_style()),
        outer[2],
    );
}

fn render_block(frame: &mut ratatui::Frame<'_>, block: Block<'_>, area: Rect) -> Rect {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn setup() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn teardown(term: &mut Term) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
