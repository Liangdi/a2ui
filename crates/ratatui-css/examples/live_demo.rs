//! Interactive live TUI — the headline demo.
//!
//! A small set of buttons. Use ←/→ to move focus and `d` to toggle the
//! `disabled` state; the CSS cascade restyles the focused/disabled buttons in
//! real time via `:focus` and `:disabled` pseudo-classes. `q`/`Esc` to quit.
//!
//! ```sh
//! cargo run -p ratatui-css --example live_demo
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
    widgets::{Block, Paragraph},
};

use ratatui_css::{OwnedNode, State, Stylesheet};

type Term = Terminal<CrosstermBackend<Stdout>>;

struct Button {
    label: &'static str,
    class: &'static str,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sheet = Stylesheet::parse(
        r##"
        :root {
            --accent: #89b4fa;
            --bg: #1e1e2e;
            --panel: #313244;
            --text: #cdd6f4;
            --muted: #6c7086;
        }

        Root    { background: var(--bg); }
        Header  { color: var(--text); background: var(--panel); font-weight: bold; }
        Panel   { color: var(--text); border: rounded; padding: 1; }
        Footer  { color: var(--muted); }
        Hint    { color: var(--accent); }

        Button          { color: var(--text); padding: 0 2; }
        Button.primary  { background: var(--accent); color: #1e1e2e; font-weight: bold; }
        Button.ghost    { border: rounded; }
        Button:focus            { background: var(--panel); color: var(--accent); }
        Button.primary:focus    { background: #b4befe; }
        Button:disabled         { color: var(--muted); }
        "##,
    )?;

    let buttons = [
        Button { label: "Open", class: "primary" },
        Button { label: "Save", class: "ghost" },
        Button { label: "Export", class: "ghost" },
        Button { label: "Delete", class: "ghost" },
    ];
    let n = buttons.len();
    let mut focus = 0usize;
    let mut disabled = false;

    let mut terminal = setup()?;
    loop {
        terminal.draw(|f| draw(f, &sheet, &buttons, focus, disabled))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Left => focus = (focus + n - 1) % n,
                KeyCode::Right | KeyCode::Tab => focus = (focus + 1) % n,
                KeyCode::Char('d') => disabled = !disabled,
                KeyCode::Char('q') | KeyCode::Esc => break,
                _ => {}
            }
        }
    }
    teardown(&mut terminal)?;
    Ok(())
}

fn draw(
    frame: &mut ratatui::Frame<'_>,
    sheet: &Stylesheet,
    buttons: &[Button],
    focus: usize,
    disabled: bool,
) {
    let area = frame.area();

    // Root background fill.
    let root = sheet.compute(&OwnedNode::new("Root"), None);
    frame.render_widget(Block::default().style(root.to_style()), area);

    let cols = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    // Header.
    let header = sheet.compute(&OwnedNode::new("Header"), None);
    frame.render_widget(
        Paragraph::new(Line::from(" ratatui-css · live cascade demo"))
            .style(header.to_style())
            .alignment(Alignment::Center),
        cols[0],
    );

    // Panel + button row.
    let panel = sheet.compute(&OwnedNode::new("Panel"), None);
    let inner = panel_block(frame, panel.to_block(), cols[1]);

    let instructions = Paragraph::new(Line::from(format!(
        " focus: {}   disabled: {} ",
        buttons[focus].label,
        if disabled { "on" } else { "off" }
    )));
    let btn_rects = button_layout(inner);
    frame.render_widget(instructions, btn_rects[0]);

    for (i, b) in buttons.iter().enumerate() {
        let node = OwnedNode::new("Button")
            .with_classes([b.class])
            .with_state(State {
                focus: i == focus,
                disabled,
                ..State::empty()
            });
        let computed = sheet.compute(&node, None);
        let para = Paragraph::new(Line::from(format!(" {} ", b.label)))
            .style(computed.to_style())
            .alignment(Alignment::Center);
        // ghost buttons get their border; primary/focus rely on bg fill.
        if let Some(rect) = btn_rects.get(i + 1) {
            if b.class == "ghost" && i != focus {
                let block = computed.to_block();
                frame.render_widget(para.block(block), *rect);
            } else {
                frame.render_widget(para, *rect);
            }
        }
    }

    // Footer hint.
    let footer = sheet.compute(&OwnedNode::new("Footer"), None);
    frame.render_widget(
        Paragraph::new(Line::from(format!(
            " {:<-10} to move focus · {:<-10} to toggle disabled · q to quit",
            "←/→", "d"
        )))
        .style(footer.to_style()),
        cols[2],
    );
}

/// Render a panel `Block`, returning the inner content area.
fn panel_block(frame: &mut ratatui::Frame<'_>, block: Block<'_>, area: Rect) -> Rect {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// Split the panel inner area into [instructions, btn0, btn1, btn2, btn3].
fn button_layout(inner: Rect) -> Vec<Rect> {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(3)])
        .split(inner);
    let mut out = vec![split[0]]; // instructions line
    let btns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .horizontal_margin(1)
        .split(split[1]);
    out.extend(btns.iter().copied());
    out
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
