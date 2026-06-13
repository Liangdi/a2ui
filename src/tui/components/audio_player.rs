//! AudioPlayer component.
//!
//! By default (no `audio` feature) it renders a text placeholder:
//! `[♫ description]`. The terminal cannot decode audio without extra native
//! dependencies.
//!
//! With the **`audio`** Cargo feature enabled, it becomes a real, interactive
//! player: playback starts from a LOCAL file path (resolved from the `url`
//! binding) and is controlled live — play/pause, volume, and replay after the
//! track ends — via keyboard when the component has focus. It also fixes the
//! earlier "re-trigger playback every frame" bug: each instance's `rodio`
//! handles live in a per-instance cache ([`player::HANDLES`]), created once on
//! first render and reused.
//!
//! # Interaction (requires `audio` feature + focus)
//! | Key | Action |
//! |-----|--------|
//! | `Space` | Play / Pause (or Replay when finished) |
//! | `↑` / `↓` | Volume ±10 % |
//!
//! Seeking is intentionally **not** supported: rodio's bundled decoders only
//! support forward seeks reliably (WAV/MP3/FLAC reject backward seeks with
//! `RandomAccessNotSupported`), so a consistent seek UX isn't achievable.
//!
//! Because the component is a stateless `&self` singleton (rendered every
//! frame), live playback state cannot live on the struct — it lives in the
//! handle cache keyed by `surface_id:component_id`. The bound data model is
//! only used for `url` / `description`, as before.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::core::model::component_context::ComponentContext;
use crate::core::protocol::common_types::DynamicString;
use crate::tui::component_impl::TuiComponent;

// Event types are only needed by the feature-gated `handle_event`.
#[cfg(feature = "audio")]
use crate::core::event::{EventResult, InputEvent, InputKey};

/// Render the standard text placeholder into `inner`.
fn render_placeholder(description: &str, display_text: &str, inner: Rect, frame: &mut Frame) {
    let placeholder = if description.is_empty() {
        format!("[\u{266B} {}]", display_text)
    } else {
        format!("[\u{266B} {} \u{2014} {}]", description, display_text)
    };
    let paragraph = Paragraph::new(Line::from(Span::styled(
        placeholder,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(paragraph, inner);
}

/// AudioPlayer component.
///
/// Placeholder-only without the `audio` feature; an interactive player with it.
/// Applies a default 1-cell margin.
pub struct AudioPlayerComponent;

impl TuiComponent for AudioPlayerComponent {
    fn name(&self) -> &'static str {
        "AudioPlayer"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        // Apply default 1-cell margin on all sides.
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Resolve url + description.
        let url = match comp_model.get_property::<DynamicString>("url") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };
        let description = comp_model
            .get_property::<DynamicString>("description")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
            .unwrap_or_default();
        let display = if !description.is_empty() {
            description.clone()
        } else if !url.is_empty() {
            url.clone()
        } else {
            "audio".to_string()
        };

        // Real interactive player (feature-gated). Falls back to the
        // placeholder when playback can't start (non-local url, missing file,
        // no audio device, decode error).
        #[cfg(feature = "audio")]
        {
            let key = player::key(&ctx.surface_id, &ctx.component_id);
            if player::ensure_started(&key, &url) {
                if let Some(snap) = player::snapshot(&key) {
                    player::draw(frame, inner, &display, &snap);
                    return;
                }
            }
        }

        render_placeholder(&description, &display, inner, frame);
    }

    /// Keyboard control of the live player (feature-gated). Without the
    /// `audio` feature the trait's default (`None`) is used.
    #[cfg(feature = "audio")]
    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &InputEvent,
    ) -> Option<EventResult> {
        // `InputEvent` has a single variant, so this destructure is
        // irrefutable; `key` binds by reference via match ergonomics.
        let InputEvent::KeyPress { key } = event;
        let op = match key {
            InputKey::Space => player::Op::Toggle,
            InputKey::Up => player::Op::VolUp,
            InputKey::Down => player::Op::VolDown,
            _ => return None,
        };
        let key = player::key(&ctx.surface_id, &ctx.component_id);
        player::control(&key, op);
        // Playback state lives in the handle cache, not the data model, so
        // there is nothing to write back — just signal that we consumed it.
        Some(EventResult::Consumed)
    }
}

// ---------------------------------------------------------------------------
// Live playback (only compiled under `feature = "audio"`)
// ---------------------------------------------------------------------------

#[cfg(feature = "audio")]
mod player {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::BufReader;
    use std::time::Duration;

    use ratatui::{
        Frame,
        layout::{Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Gauge, Paragraph},
    };
    use rodio::{Decoder, MixerDeviceSink, Player, Source};

    /// One live playback session for a component instance.
    struct Handle {
        // Order matters for drop: `player` drops before `sink`. The sink is
        // held only to keep the audio device alive (never read here).
        #[allow(dead_code)]
        sink: MixerDeviceSink,
        player: Player,
        url: String,
        total: Option<Duration>,
    }

    thread_local! {
        /// Per-instance live handles, keyed by `surface_id:component_id`.
        /// `thread_local` + `RefCell` because the TUI is single-threaded and
        /// this avoids requiring the rodio handles to be `Send`.
        static HANDLES: RefCell<HashMap<String, Handle>> = RefCell::new(HashMap::new());
    }

    /// A cheap point-in-time read of a session; copied out of the borrow so
    /// it can be used while drawing without holding the `RefCell`.
    #[derive(Clone, Copy, Default)]
    pub(crate) struct Snapshot {
        paused: bool,
        ended: bool,
        pos: Duration,
        vol: f32,
        total: Option<Duration>,
    }

    /// A control operation requested by a key press.
    pub(crate) enum Op {
        Toggle,
        VolUp,
        VolDown,
    }

    /// Stable per-instance cache key.
    pub(crate) fn key(surface_id: &str, component_id: &str) -> String {
        format!("{surface_id}:{component_id}")
    }

    /// Open the device, decode `url`, and build a live `Handle`. Local file
    /// paths only (no HTTP fetch).
    fn open(url: &str) -> Result<Handle, ()> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink().map_err(|_| ())?;
        // Silence the "Dropping DeviceSink" stderr notice — this app uses
        // stderr as the TUI backend and stray output corrupts the screen.
        sink.log_on_drop(false);
        let file = File::open(url).map_err(|_| ())?;
        let decoder = Decoder::new(BufReader::new(file)).map_err(|_| ())?;
        let total = decoder.total_duration();
        let player = Player::connect_new(sink.mixer());
        player.append(decoder);
        Ok(Handle {
            sink,
            player,
            url: url.to_string(),
            total,
        })
    }

    /// Ensure a session exists for `key` playing `url`. Creates one if absent
    /// or if the URL changed (server swapped the track). Returns `false` if
    /// playback could not start, so the caller falls back to the placeholder.
    pub(crate) fn ensure_started(key: &str, url: &str) -> bool {
        if url.is_empty() || url.starts_with("http://") || url.starts_with("https://") {
            return false;
        }
        if !std::path::Path::new(url).is_file() {
            return false;
        }
        HANDLES.with(|m| -> bool {
            let mut m = m.borrow_mut();
            let needs = m.get(key).map_or(true, |h| h.url != url);
            if needs {
                match open(url) {
                    Ok(h) => {
                        m.insert(key.to_string(), h);
                        true
                    }
                    Err(()) => false,
                }
            } else {
                true
            }
        })
    }

    /// Read the current playback state, if a session exists for `key`.
    pub(crate) fn snapshot(key: &str) -> Option<Snapshot> {
        HANDLES.with(|m| {
            m.borrow().get(key).map(|h| Snapshot {
                paused: h.player.is_paused(),
                ended: h.player.empty(),
                pos: h.player.get_pos(),
                vol: h.player.volume(),
                total: h.total,
            })
        })
    }

    /// Apply a control operation to the session for `key` (no-op if absent).
    /// `Toggle` resumes when paused, pauses when playing, and — when the track
    /// has finished — replays it from the start (re-decode + append to the
    /// same player).
    pub(crate) fn control(key: &str, op: Op) {
        HANDLES.with(|m| {
            let mut m = m.borrow_mut();
            let Some(h) = m.get_mut(key) else { return };
            match op {
                Op::Toggle => {
                    if h.player.empty() {
                        if let Ok(file) = File::open(&h.url) {
                            if let Ok(dec) = Decoder::new(BufReader::new(file)) {
                                h.player.append(dec);
                            }
                        }
                        h.player.play();
                    } else if h.player.is_paused() {
                        h.player.play();
                    } else {
                        h.player.pause();
                    }
                }
                Op::VolUp => h.player.set_volume((h.player.volume() + 0.1).min(1.0)),
                Op::VolDown => h.player.set_volume((h.player.volume() - 0.1).max(0.0)),
            }
        });
    }

    fn fmt_dur(d: Duration) -> String {
        let s = d.as_secs();
        format!("{}:{:02}", s / 60, s % 60)
    }

    /// Draw the player UI into `area` from a `Snapshot`.
    pub(crate) fn draw(frame: &mut Frame, area: Rect, display: &str, snap: &Snapshot) {
        let (icon, label, color) = if snap.ended {
            ("\u{25A0}", "Ended", Color::DarkGray)
        } else if snap.paused {
            ("\u{23F8}", "Paused", Color::Yellow)
        } else {
            ("\u{25B6}", "Playing", Color::Green)
        };

        // Degrade to a single status line when there isn't room for gauges.
        if area.height < 4 || area.width < 12 {
            let p = Paragraph::new(format!("{icon} {label} \u{2014} {display}"));
            frame.render_widget(p, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // state
                Constraint::Length(1), // progress
                Constraint::Length(1), // volume
                Constraint::Length(1), // hints
            ])
            .split(area);

        let state = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{icon} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(label),
            Span::raw(format!("  \u{2014} {display}")),
        ]));
        frame.render_widget(state, chunks[0]);

        match snap.total {
            Some(t) => {
                let pct =
                    ((snap.pos.as_secs_f64() / t.as_secs_f64()) * 100.0).clamp(0.0, 100.0) as u16;
                let g = Gauge::default()
                    .gauge_style(Style::default().fg(Color::Cyan))
                    .percent(pct)
                    .label(format!("{} / {}", fmt_dur(snap.pos), fmt_dur(t)));
                frame.render_widget(g, chunks[1]);
            }
            None => {
                let p = Paragraph::new(format!("{}  (duration unknown)", fmt_dur(snap.pos)));
                frame.render_widget(p, chunks[1]);
            }
        }

        let vpct = (snap.vol * 100.0).round().clamp(0.0, 100.0) as u16;
        let vg = Gauge::default()
            .gauge_style(Style::default().fg(Color::Magenta))
            .percent(vpct)
            .label(format!("Vol {}%", vpct));
        frame.render_widget(vg, chunks[2]);

        let hint = Paragraph::new(Line::from(
            " Space:play/pause/replay   \u{2191}/\u{2193}:volume",
        ))
        .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, chunks[3]);
    }
}
