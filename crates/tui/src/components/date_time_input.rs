//! DateTimeInput component — renders a date/time input display.

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime, Timelike};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use a2ui_core::event::{EventResult, InputEvent, InputKey};
use a2ui_core::model::component_context::ComponentContext;
use a2ui_core::protocol::common_types::DynamicString;
use crate::component_impl::TuiComponent;

/// DateTimeInput component implementation.
///
/// Renders a bordered block with a date/time icon and the ISO date string.
/// Display: `[📅 2026-06-13 14:30]` style.
/// Applies a default 1-cell margin.
pub struct DateTimeInputComponent;

impl TuiComponent for DateTimeInputComponent {
    fn name(&self) -> &'static str {
        "DateTimeInput"
    }

    fn render(
        &self,
        ctx: &ComponentContext,
        area: Rect,
        frame: &mut Frame,
        _render_child: &mut dyn FnMut(&str, Rect, &mut Frame, &str),
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) {
        let comp_model = match ctx.components.get(&ctx.component_id) {
            Some(m) => m,
            None => return,
        };

        // Apply default 1-cell margin on all sides (never collapses to zero).
        let inner = crate::layout_engine::padded_content(area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Resolve label.
        let label = match comp_model.get_property::<DynamicString>("label") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve value (ISO date string).
        let value = match comp_model.get_property::<DynamicString>("value") {
            Some(ds) => ctx.data_context.resolve_dynamic_string(&ds),
            None => String::new(),
        };

        // Resolve enableDate and enableTime flags.
        let enable_date: bool = comp_model.get_property("enableDate").unwrap_or(true);
        let enable_time: bool = comp_model.get_property("enableTime").unwrap_or(true);
        let _min = comp_model.get_property::<DynamicString>("min")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds));
        let _max = comp_model.get_property::<DynamicString>("max")
            .map(|ds| ctx.data_context.resolve_dynamic_string(&ds));

        // Choose icon based on enabled modes.
        let icon = match (enable_date, enable_time) {
            (true, true) => "\u{1F4C5}",   // calendar
            (true, false) => "\u{1F4C5}",   // calendar only
            (false, true) => "\u{23F0}",    // clock only
            (false, false) => "\u{1F4C5}",  // default
        };

        // Build display text with appropriate icon.
        let display_text = format!("{} {}", icon, value);

        // Determine if this date-time input has keyboard focus.
        let is_focused = ctx.focused_id.as_deref() == Some(ctx.component_id.as_str());

        // Build bordered block with label as title.
        let block_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let mut block = Block::default().borders(Borders::ALL).style(block_style);
        if !label.is_empty() {
            block = block.title(Span::styled(
                format!(" {} ", label),
                Style::default().fg(Color::White),
            ));
        }

        let content_area = block.inner(inner);
        frame.render_widget(block, inner);

        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        let paragraph = Paragraph::new(Line::from(Span::styled(
            display_text,
            Style::default().fg(Color::White),
        )));
        frame.render_widget(paragraph, content_area);
    }

    fn natural_height(
        &self,
        _ctx: &ComponentContext,
        _available_width: u16,
        _measure_child: &mut dyn FnMut(&str, &str, u16) -> Option<u16>,
    ) -> Option<u16> {
        // 1 display line + 2-cell margin + 2-cell border = 5 (render shrink(1) margin
        // then a bordered block, so content needs area.height - 4 >= 1).
        Some(5)
    }

    fn handle_event(
        &self,
        ctx: &ComponentContext,
        event: &a2ui_core::event::InputEvent,
    ) -> Option<a2ui_core::event::EventResult> {
        let comp_model = ctx.components.get(&ctx.component_id)?;

        // The value must be a data binding — otherwise there is no path in the
        // data model to write the new datetime back to (mirrors slider.rs).
        let value_ds = comp_model.get_property::<DynamicString>("value")?;
        let binding = match value_ds {
            DynamicString::Binding(b) => b,
            _ => return None,
        };

        // Resolve flags (default both enabled, matching render()).
        let enable_date: bool = comp_model.get_property("enableDate").unwrap_or(true);
        let enable_time: bool = comp_model.get_property("enableTime").unwrap_or(true);

        // Read + parse the current value. On empty/unparseable input, seed with now.
        let current_str =
            ctx.data_context.resolve_dynamic_string(&DynamicString::Binding(binding.clone()));
        let dt = parse_value(&current_str).unwrap_or_else(|| chrono::Local::now().naive_local());

        // Only the four arrow keys are handled; everything else bubbles up.
        let direction = match event {
            InputEvent::KeyPress { key: InputKey::Up } => Direction::Forward,
            InputEvent::KeyPress { key: InputKey::Right } => Direction::Forward,
            InputEvent::KeyPress { key: InputKey::Down } => Direction::Backward,
            InputEvent::KeyPress { key: InputKey::Left } => Direction::Backward,
            _ => return None,
        };
        let axis = match event {
            InputEvent::KeyPress { key: InputKey::Up } | InputEvent::KeyPress { key: InputKey::Down } => Axis::Primary,
            InputEvent::KeyPress { key: InputKey::Left } | InputEvent::KeyPress { key: InputKey::Right } => Axis::Secondary,
            _ => return None,
        };

        let new_dt = apply_delta(dt, enable_date, enable_time, axis, direction);
        let formatted = format_value(&new_dt, enable_date, enable_time);

        Some(EventResult::DataUpdate {
            path: binding.path.clone(),
            value: serde_json::json!(formatted),
        })
    }
}

/// Which axis an arrow key maps to (Up/Down = primary, Left/Right = secondary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Primary,
    Secondary,
}

/// Whether the increment should be positive or negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Forward,
    Backward,
}

/// Parse an ISO datetime string into a [`NaiveDateTime`].
///
/// Accepts full datetime (`YYYY-MM-DDTHH:MM:SS`), date-only (`YYYY-MM-DD`),
/// and time-only (`HH:MM:SS`) shapes.
fn parse_value(value: &str) -> Option<NaiveDateTime> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Full ISO datetime (optionally with fractional seconds / space separator).
    NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f"))
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S"))
        .ok()
        // Date-only: anchor to midnight.
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
        // Time-only: anchor to today's date.
        .or_else(|| {
            NaiveTime::parse_from_str(trimmed, "%H:%M:%S")
                .ok()
                .map(|t| chrono::Local::now().naive_local().date().and_time(t))
        })
}

/// Apply an increment to a datetime based on the enabled modes and the key axis.
///
/// Increment rules:
/// - `enableDate && enableTime`: primary(Up/Down) = ±1 day, secondary(Left/Right) = ±1 hour
/// - `enableDate` only: primary = ±1 day, secondary = ±1 month
/// - `enableTime` only: primary = ±1 minute, secondary = ±1 hour
///
/// Month arithmetic is clamped to the last valid day of the target month
/// (chrono's behavior on `with_month` overflow would otherwise return `None`).
fn apply_delta(
    dt: NaiveDateTime,
    enable_date: bool,
    enable_time: bool,
    axis: Axis,
    direction: Direction,
) -> NaiveDateTime {
    let sign: i64 = if direction == Direction::Forward { 1 } else { -1 };

    let duration_step = |days: i64, secs: i64| -> NaiveDateTime {
        dt + Duration::days(days) + Duration::seconds(secs)
    };

    match (enable_date, enable_time) {
        (true, true) => match axis {
            Axis::Primary => duration_step(sign, 0),            // ±1 day
            Axis::Secondary => duration_step(0, sign * 3600),   // ±1 hour
        },
        (true, false) => match axis {
            Axis::Primary => duration_step(sign, 0),            // ±1 day
            Axis::Secondary => add_months(dt, sign),            // ±1 month
        },
        (false, true) => match axis {
            Axis::Primary => duration_step(0, sign * 60),       // ±1 minute
            Axis::Secondary => duration_step(0, sign * 3600),   // ±1 hour
        },
        // Neither enabled is a degenerate config; fall back to day stepping so the
        // key still does *something* rather than silently swallowing the event.
        (false, false) => duration_step(sign, 0),
    }
}

/// Add (or subtract) `n` months from a datetime, clamping the day to the
/// last valid day of the resulting month (e.g. Jan 31 -> Feb 28/29).
fn add_months(dt: NaiveDateTime, n: i64) -> NaiveDateTime {
    let date = dt.date();
    let year = date.year() as i64;
    let month = date.month() as i64;
    let day = date.day();

    let total = year * 12 + (month - 1) + n;
    let new_year = total.div_euclid(12) as i32;
    let new_month = total.rem_euclid(12) as u32 + 1;

    let last_day = days_in_month(new_year, new_month);
    let clamped_day = day.min(last_day);

    match chrono::NaiveDate::from_ymd_opt(new_year, new_month, clamped_day) {
        Some(d) => d.and_hms_opt(dt.hour(), dt.minute(), dt.second())
            .unwrap_or(dt),
        None => dt,
    }
}

/// Number of days in a given month, accounting for leap years.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Whether `year` is a leap year under the Gregorian calendar.
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Format a datetime back to the ISO shape appropriate for the enabled modes.
///
/// - `!enable_time`: `YYYY-MM-DD` (date-only)
/// - `!enable_date`: `HH:MM:SS` (time-only)
/// - both: `YYYY-MM-DDTHH:MM:SS`
fn format_value(dt: &NaiveDateTime, enable_date: bool, enable_time: bool) -> String {
    match (enable_date, enable_time) {
        (true, true) => dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
        (true, false) => dt.format("%Y-%m-%d").to_string(),
        (false, true) => dt.format("%H:%M:%S").to_string(),
        (false, false) => dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(year: i32, month: u32, day: u32, h: u32, m: u32, s: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    #[test]
    fn parse_full_iso_datetime() {
        let parsed = parse_value("2026-06-13T14:30:00").unwrap();
        assert_eq!(parsed, dt(2026, 6, 13, 14, 30, 0));
    }

    #[test]
    fn parse_date_only_anchors_midnight() {
        let parsed = parse_value("2026-01-15").unwrap();
        assert_eq!(parsed, dt(2026, 1, 15, 0, 0, 0));
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(parse_value("").is_none());
        assert!(parse_value("   ").is_none());
        assert!(parse_value("not-a-date").is_none());
    }

    #[test]
    fn date_and_time_mode_day_increment() {
        let start = dt(2026, 6, 13, 14, 30, 0);
        // Up -> +1 day
        let after = apply_delta(start, true, true, Axis::Primary, Direction::Forward);
        assert_eq!(after, dt(2026, 6, 14, 14, 30, 0));
        // Down -> -1 day (back to start)
        let back = apply_delta(after, true, true, Axis::Primary, Direction::Backward);
        assert_eq!(back, start);
    }

    #[test]
    fn date_and_time_mode_hour_increment() {
        let start = dt(2026, 6, 13, 14, 30, 0);
        // Right -> +1 hour
        let after = apply_delta(start, true, true, Axis::Secondary, Direction::Forward);
        assert_eq!(after, dt(2026, 6, 13, 15, 30, 0));
        // Left -> -1 hour
        let back = apply_delta(after, true, true, Axis::Secondary, Direction::Backward);
        assert_eq!(back, start);
    }

    #[test]
    fn date_and_time_hour_wraps_across_day() {
        let start = dt(2026, 6, 13, 23, 30, 0);
        let after = apply_delta(start, true, true, Axis::Secondary, Direction::Forward);
        assert_eq!(after, dt(2026, 6, 14, 0, 30, 0));
    }

    #[test]
    fn date_only_mode_month_increment() {
        let start = dt(2026, 1, 15, 0, 0, 0);
        // Right -> +1 month
        let after = apply_delta(start, true, false, Axis::Secondary, Direction::Forward);
        assert_eq!(after, dt(2026, 2, 15, 0, 0, 0));
        // Left -> -1 month
        let back = apply_delta(after, true, false, Axis::Secondary, Direction::Backward);
        assert_eq!(back, start);
    }

    #[test]
    fn date_only_month_clamps_jan31_to_feb() {
        let start = dt(2026, 1, 31, 0, 0, 0);
        let after = apply_delta(start, true, false, Axis::Secondary, Direction::Forward);
        // 2026 is not a leap year, so Feb has 28 days.
        assert_eq!(after, dt(2026, 2, 28, 0, 0, 0));
    }

    #[test]
    fn date_only_month_clamps_to_leap_feb() {
        // 2024 is a leap year.
        let start = dt(2024, 1, 31, 0, 0, 0);
        let after = apply_delta(start, true, false, Axis::Secondary, Direction::Forward);
        assert_eq!(after, dt(2024, 2, 29, 0, 0, 0));
    }

    #[test]
    fn date_only_month_wraps_across_year() {
        let start = dt(2026, 12, 15, 0, 0, 0);
        let after = apply_delta(start, true, false, Axis::Secondary, Direction::Forward);
        assert_eq!(after, dt(2027, 1, 15, 0, 0, 0));
        // Backward one month from December 2026 is November 2026 (same year).
        let back = apply_delta(start, true, false, Axis::Secondary, Direction::Backward);
        assert_eq!(back, dt(2026, 11, 15, 0, 0, 0));
        // A genuine year-wrap backward: January minus one month.
        let jan = dt(2026, 1, 15, 0, 0, 0);
        let prev_year = apply_delta(jan, true, false, Axis::Secondary, Direction::Backward);
        assert_eq!(prev_year, dt(2025, 12, 15, 0, 0, 0));
    }

    #[test]
    fn time_only_mode_minute_increment() {
        let start = dt(2026, 6, 13, 14, 30, 0);
        // Up -> +1 minute
        let after = apply_delta(start, false, true, Axis::Primary, Direction::Forward);
        assert_eq!(after, dt(2026, 6, 13, 14, 31, 0));
        // Down -> -1 minute
        let back = apply_delta(after, false, true, Axis::Primary, Direction::Backward);
        assert_eq!(back, start);
    }

    #[test]
    fn time_only_mode_hour_increment() {
        let start = dt(2026, 6, 13, 14, 30, 0);
        // Right -> +1 hour
        let after = apply_delta(start, false, true, Axis::Secondary, Direction::Forward);
        assert_eq!(after, dt(2026, 6, 13, 15, 30, 0));
    }

    #[test]
    fn format_full_datetime() {
        let value = dt(2026, 6, 13, 14, 30, 5);
        assert_eq!(format_value(&value, true, true), "2026-06-13T14:30:05");
    }

    #[test]
    fn format_date_only() {
        let value = dt(2026, 6, 13, 14, 30, 5);
        assert_eq!(format_value(&value, true, false), "2026-06-13");
    }

    #[test]
    fn format_time_only() {
        let value = dt(2026, 6, 13, 14, 30, 5);
        assert_eq!(format_value(&value, false, true), "14:30:05");
    }

    #[test]
    fn leap_year_detection() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2026));
        assert!(!is_leap_year(1900)); // divisible by 100 but not 400
        assert!(is_leap_year(2000)); // divisible by 400
    }
}
