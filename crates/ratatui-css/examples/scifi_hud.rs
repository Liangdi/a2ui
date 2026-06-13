//! Sci-fi HUD — a cyberpunk heads-up-display rendered from a CSS stylesheet.
//!
//! Neon palette via CSS tokens, double-line frames, a sweeping radar, live
//! telemetry bars, and an event log. A self-incrementing tick drives a few
//! animations (radar sweep, oscillating gauges, spinner) so the screen feels
//! alive. `q`/`Esc` to quit.
//!
//! ```sh
//! cargo run -p ratatui-css --example scifi_hud
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
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph},
};

use ratatui_css::{OwnedNode, Stylesheet};

type Term = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sheet = Stylesheet::parse(
        r##"
        :root {
            --cyan: #56f0ff;
            --magenta: #ff4fd8;
            --green: #5dffb0;
            --amber: #ffb454;
            --bg: #04111a;
            --panel: #07222e;
            --dim: #2a6a7a;
            --text: #b8f2ff;
        }

        Root         { background: var(--bg); }
        Header       { color: var(--cyan); background: var(--panel); font-weight: bold; }
        Footer       { color: var(--dim); }
        Frame        { color: var(--dim); border: double; padding: 1; }
        Frame.main   { color: var(--cyan); border: double; padding: 1; }

        Label        { color: var(--dim); }
        Value        { color: var(--text); }
        Value.ok     { color: var(--green); }
        Value.warn   { color: var(--amber); }
        Value.alert  { color: var(--magenta); }

        Scan         { color: var(--cyan); }
        Target       { color: var(--magenta); }
        Bar          { color: var(--green); }
        Bar.warn     { color: var(--amber); }
        Bar.alert    { color: var(--magenta); }
        Event        { color: var(--dim); }
        Event.new    { color: var(--amber); }
        "##,
    )?;

    let mut terminal = setup()?;
    let mut tick: u32 = 0;
    loop {
        terminal.draw(|f| draw(f, &sheet, tick))?;
        tick = tick.wrapping_add(1);

        if event::poll(Duration::from_millis(80))?
            && let Event::Key(key) = event::read()?
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            break;
        }
    }
    teardown(&mut terminal)?;
    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, sheet: &Stylesheet, tick: u32) {
    let area = frame.area();

    // Root background.
    let root = sheet.compute(&OwnedNode::new("Root"), None);
    frame.render_widget(Block::default().style(root.to_style()), area);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    // --- Header: title + animated status ---
    let header = sheet.compute(&OwnedNode::new("Header"), None);
    let spinner = ['|', '/', '—', '\\'][((tick / 3) as usize) % 4];
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" ⟁ A2UI // TACTICAL HUD"),
            Span::raw(format!(
                "{:>width$}",
                format!("SYS {}  ● ONLINE", spinner),
                width = (outer[0].width as usize).saturating_sub(2)
            )),
        ]))
        .style(header.to_style()),
        outer[0],
    );

    // --- Body: telemetry | scanner | events ---
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(20), Constraint::Length(30)])
        .split(outer[1]);

    telemetry_panel(frame, sheet, body[0], tick);
    scanner_panel(frame, sheet, body[1], tick);
    events_panel(frame, sheet, body[2], tick);

    // --- Footer ---
    let footer = sheet.compute(&OwnedNode::new("Footer"), None);
    frame.render_widget(
        Paragraph::new(" [q] exit").style(footer.to_style()),
        outer[2],
    );
}

fn telemetry_panel(frame: &mut ratatui::Frame<'_>, sheet: &Stylesheet, area: Rect, tick: u32) {
    let panel = sheet.compute(&OwnedNode::new("Frame").with_classes(["main"]), None);
    let inner = render_block(frame, panel.to_block().title(" ◈ TELEMETRY "), area);

    let gauges = [
        ("CORE", 55.0 + (tick as f64 * 0.07).sin() * 18.0, 0),
        ("PWR", 78.0 + (tick as f64 * 0.05).sin() * 10.0, 0),
        ("HULL", 40.0 + (tick as f64 * 0.03).sin() * 35.0, 0),
        ("SHLD", 20.0 + (tick as f64 * 0.09).sin() * 60.0, 0),
    ];

    let mut lines: Vec<Line<'_>> = Vec::new();
    for (label, value, _) in &gauges {
        let pct = value.clamp(0.0, 100.0);
        let cls = if pct < 30.0 { "alert" } else if pct < 60.0 { "warn" } else { "" };
        let bar_style = sheet
            .compute(&OwnedNode::new("Bar").with_classes([cls]), None)
            .to_style();
        let label_style = sheet.compute(&OwnedNode::new("Label"), None).to_style();
        let val_style = sheet.compute(&OwnedNode::new("Value").with_classes([cls]), None).to_style();

        let filled = ((pct / 100.0) * 14.0).round() as usize;
        let bar = format!("{}{}", "▰".repeat(filled), "▱".repeat(14 - filled));
        let mut line = Line::default();
        line.push_span(Span::raw(format!(" {label:<4} ")).style(label_style));
        line.push_span(Span::raw(bar).style(bar_style));
        line.push_span(Span::raw(format!(" {:3.0}%", pct)).style(val_style));
        lines.push(line);
    }
    frame.render_widget(List::new(lines), inner);
}

fn scanner_panel(frame: &mut ratatui::Frame<'_>, sheet: &Stylesheet, area: Rect, tick: u32) {
    let panel = sheet.compute(&OwnedNode::new("Frame").with_classes(["main"]), None);
    let inner = render_block(frame, panel.to_block().title(" ◎ SCANNER "), area);

    let scan = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);

    // Radar grid.
    let grid_style = sheet.compute(&OwnedNode::new("Scan"), None).to_style();
    let target_style = sheet.compute(&OwnedNode::new("Target"), None).to_style();
    let width = scan[0].width.max(2) as usize;
    let height = scan[0].height as usize;
    let (cx, cy) = ((width as i32) / 2, (height as i32) / 2);
    let radius = (cx.min(cy) - 1).max(1) as f64;

    let sweep_angle = (tick as f64) * 0.20;
    let tip_x = cx as f64 + sweep_angle.cos() * radius;
    let tip_y = cy as f64 + sweep_angle.sin() * radius * 0.5; // squash for char aspect

    let mut grid_lines: Vec<Line<'_>> = Vec::new();
    for y in 0..height {
        let mut spans: Vec<Span<'_>> = Vec::new();
        for x in 0..width {
            let (dx, dy) = (x as i32 - cx, y as i32 - cy);
            let ch = if dx == 0 && dy == 0 {
                Some(('✛', Some(target_style)))
            } else if (x as f64 - tip_x).abs() < 0.6 && (y as f64 - tip_y).abs() < 0.6 {
                Some(('●', Some(grid_style)))
            } else if dx.abs() == dy.abs() && dx.abs() == (radius as i32).max(1) {
                Some(('·', None))
            } else {
                Some((' ', None))
            };
            if let Some((c, st)) = ch {
                spans.push(Span::raw(c.to_string()).style(st.unwrap_or_default()));
            }
        }
        grid_lines.push(Line::from(spans));
    }
    frame.render_widget(List::new(grid_lines), scan[0]);

    // Target lock readout.
    let lock_angle = (sweep_angle * 57.2958) % 360.0;
    let dist = (radius * 1.7) as u32;
    let value = sheet.compute(&OwnedNode::new("Value"), None).to_style();
    let label = sheet.compute(&OwnedNode::new("Label"), None).to_style();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" BEARING ").style(label),
            Span::raw(format!("{lock_angle:5.1}°")).style(value),
            Span::raw("   RANGE ").style(label),
            Span::raw(format!("{dist:4}m")).style(value),
        ]))
        .alignment(Alignment::Center),
        scan[1],
    );
}

fn events_panel(frame: &mut ratatui::Frame<'_>, sheet: &Stylesheet, area: Rect, tick: u32) {
    let panel = sheet.compute(&OwnedNode::new("Frame"), None);
    let inner = render_block(frame, panel.to_block().title(" ▤ EVENT LOG "), area);

    let fresh = (tick % 12) < 4;
    let events: &[(&str, &str)] = &[
        ("DOCK SEQUENCE OK", "ok"),
        ("RADAR SWEEP DONE", ""),
        ("HULL STRESS +12%", "warn"),
        ("LINK ESTABLISHED", "ok"),
        ("UNKNOWN SIGNATURE", "alert"),
        ("CALIBRATING GYRO", ""),
    ];
    let items: Vec<ListItem<'_>> = events
        .iter()
        .enumerate()
        .map(|(i, (msg, cls))| {
            let class = if i == 0 && fresh { "new" } else { *cls };
            let st = sheet
                .compute(&OwnedNode::new("Event").with_classes([class]), None)
                .to_style();
            ListItem::new(Line::from(format!(" › {msg}")).style(st))
        })
        .collect();
    frame.render_widget(List::new(items), inner);
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
