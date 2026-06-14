//! Layout calculation for Row / Column containers.
//!
//! Provides flex-grow–style weighted splitting, justify (main-axis), and
//! align (cross-axis) helpers that work on [`ratatui::layout::Rect`].

use ratatui::layout::{Direction, Rect};

use a2ui_core::protocol::common_types::{Align, Justify};

/// Content rect inside the standard 1-cell margin, but guaranteed never to collapse
/// to zero — so a leaf nested in a tight area (e.g. a `Text` label inside a `Button`'s
/// 1-row content area) still renders instead of vanishing.
///
/// - `area` height/width ≥ 3 → shrink by 1 cell on every side (normal margin).
/// - `area` height/width ≤ 2 → use the full axis (no margin; content fills it).
pub fn padded_content(area: Rect) -> Rect {
    let h = if area.height > 2 { area.height - 2 } else { area.height };
    let w = if area.width > 2 { area.width - 2 } else { area.width };
    let y = if area.height > 2 { area.y + 1 } else { area.y };
    let x = if area.width > 2 { area.x + 1 } else { area.x };
    Rect { x, y, width: w, height: h }
}

/// Split a [`Rect`] into `n` segments based on optional weights.
///
/// * If **all** weights are `None`, the area is split into equal parts.
/// * If **some** weights are set and others are `None`, the unweighted items
///   each receive a default size of 1.0 unit. The remaining space is then
///   distributed proportionally among the weighted items.
/// * If **all** items have weights, the full area is distributed proportionally.
///
/// Returns a `Vec<Rect>` with `weights.len()` entries.
pub fn weighted_split(
    direction: Direction,
    area: Rect,
    weights: &[Option<f64>],
) -> Vec<Rect> {
    let n = weights.len();
    if n == 0 {
        return vec![];
    }

    let total_size = match direction {
        Direction::Horizontal => area.width as u16,
        Direction::Vertical => area.height as u16,
    } as f64;

    // Treat None weights as 1.0 (baseline unit).
    let effective: Vec<f64> = weights.iter().map(|w| w.unwrap_or(1.0)).collect();
    let total_weight: f64 = effective.iter().sum();

    if total_weight <= 0.0 {
        // Degenerate case: return equal splits.
        return equal_split(direction, area, n);
    }

    let mut rects = Vec::with_capacity(n);
    let mut offset: u16 = 0;

    for (i, &w) in effective.iter().enumerate() {
        let fraction = w / total_weight;
        let raw = total_size * fraction;
        let mut size = raw.floor() as u16;

        // Give the last item the remainder to avoid sub-pixel gaps.
        if i == n - 1 {
            let used: u16 = rects.iter().map(|r: &Rect| size_axis(r, direction)).sum();
            size = total_size as u16 - used;
        }

        let rect = make_rect(direction, area, offset, size);
        rects.push(rect);
        offset += size;
    }

    rects
}

/// Position items along the main axis according to a [`Justify`] rule.
///
/// `items` is a list of `(Rect, u16)` pairs where the `u16` is the
/// **natural size** (width for Horizontal, height for Vertical) of each item.
/// The function returns a new `Vec<Rect>` with adjusted x/y offsets.
pub fn apply_justify(
    justify: Justify,
    items: &[(Rect, u16)],
    total_area: Rect,
    direction: Direction,
) -> Vec<Rect> {
    let container_size = size_from_direction(total_area, direction);
    let total_item_size: u16 = items.iter().map(|(_, s)| *s).sum();

    match justify {
        Justify::Start => {
            // Default layout — items are already packed to the start.
            items.iter().map(|(rect, _)| *rect).collect()
        }
        Justify::Center => {
            let gap = container_size.saturating_sub(total_item_size);
            let offset = gap / 2;
            shift_items(items, total_area, direction, offset)
        }
        Justify::End => {
            let gap = container_size.saturating_sub(total_item_size);
            shift_items(items, total_area, direction, gap)
        }
        Justify::SpaceBetween => {
            let count = items.len();
            if count <= 1 {
                return items.iter().map(|(rect, _)| *rect).collect();
            }
            let gap = container_size.saturating_sub(total_item_size);
            let spacing = gap / (count as u16 - 1);
            let mut result = Vec::with_capacity(count);
            let mut offset: u16 = 0;
            for (rect, size) in items {
                result.push(set_offset(*rect, total_area, direction, offset));
                offset += size + spacing;
            }
            result
        }
        Justify::SpaceAround => {
            let count = items.len();
            if count == 0 {
                return vec![];
            }
            let gap = container_size.saturating_sub(total_item_size);
            let spacing = gap / count as u16;
            let start_offset = spacing / 2;
            let mut result = Vec::with_capacity(count);
            let mut offset: u16 = 0;
            for (rect, size) in items {
                offset += if result.is_empty() { start_offset } else { spacing };
                result.push(set_offset(*rect, total_area, direction, offset));
                offset += size;
            }
            result
        }
        Justify::SpaceEvenly => {
            let count = items.len();
            if count == 0 {
                return vec![];
            }
            let gap = container_size.saturating_sub(total_item_size);
            let spacing = gap / (count as u16 + 1);
            let mut result = Vec::with_capacity(count);
            let mut offset: u16 = spacing;
            for (rect, size) in items {
                result.push(set_offset(*rect, total_area, direction, offset));
                offset += size + spacing;
            }
            result
        }
        Justify::Stretch => {
            let count = items.len();
            if count == 0 {
                return vec![];
            }
            let each_size = container_size / count as u16;
            let mut result = Vec::with_capacity(count);
            let mut offset: u16 = 0;
            for (i, (_rect, _size)) in items.iter().enumerate() {
                let size = if i == count - 1 {
                    container_size - offset
                } else {
                    each_size
                };
                result.push(make_rect(direction, total_area, offset, size));
                offset += size;
            }
            result
        }
    }
}

/// Position a single item on the cross axis according to an [`Align`] rule.
///
/// Returns the adjusted [`Rect`].
pub fn apply_align(align: Align, item: Rect, container: Rect, direction: Direction) -> Rect {
    let (cross_size, container_cross) = match direction {
        Direction::Horizontal => (item.height, container.height),
        Direction::Vertical => (item.width, container.width),
    };

    match align {
        Align::Start => item,
        Align::Center => {
            let offset = container_cross.saturating_sub(cross_size) / 2;
            set_cross_offset(item, container, direction, offset)
        }
        Align::End => {
            let offset = container_cross.saturating_sub(cross_size);
            set_cross_offset(item, container, direction, offset)
        }
        Align::Stretch => {
            // Expand the item to fill the cross axis, starting at the container origin.
            match direction {
                Direction::Horizontal => Rect {
                    x: item.x,
                    y: container.y,
                    width: item.width,
                    height: container.height,
                },
                Direction::Vertical => Rect {
                    x: container.x,
                    y: item.y,
                    width: container.width,
                    height: item.height,
                },
            }
        }
    }
}

/// Flexbox-style layout along the main axis.
///
/// Each item is `(natural_height, explicit_weight)`:
/// - `natural_height = Some(h)` → the component has a content-driven base of `h`.
/// - `natural_height = None` → "no opinion": treated as a legacy fill participant
///   (base 0, implicit weight `1.0`), reproducing the old [`weighted_split`] behavior
///   so unconverted/`None` components are laid out exactly as before.
///
/// An explicit `weight` overrides the implicit one and acts as **flex-grow**: leftover
/// space (after every measured child gets its natural height) is distributed
/// proportionally to weighted items. Unweighted measured items keep their natural
/// height. When there are no weights and `justify` is [`Justify::Stretch`], every item
/// grows equally to fill the axis.
///
/// Returns positioned, main-axis-sized `Rect`s. The caller still applies cross-axis
/// alignment via [`apply_align`]. **When `justify` is [`Justify::Stretch`], the main
/// axis is fully consumed here — do not also call [`apply_justify`] with `Stretch`.**
///
/// Arithmetic uses `i64` internally so that overflow (content taller than the area)
/// and sub-pixel rounding never underflow `u16`.
pub fn flex_layout(
    direction: Direction,
    area: Rect,
    items: &[(Option<u16>, Option<f64>)],
    justify: Justify,
) -> Vec<Rect> {
    let n = items.len();
    if n == 0 {
        return vec![];
    }

    let total = size_from_direction(area, direction) as i64;

    // Resolve base and effective weight per item.
    let bases: Vec<i64> = items.iter().map(|(nat, _)| nat.unwrap_or(0) as i64).collect();
    let weights: Vec<f64> = items
        .iter()
        .map(|(nat, w)| w.unwrap_or(if nat.is_none() { 1.0 } else { 0.0 }))
        .collect();

    let sum_base: i64 = bases.iter().sum();
    let sum_weight: f64 = weights.iter().sum();
    let free = total - sum_base; // may be negative (overflow)

    let mut finals: Vec<i64> = vec![0; n];

    if free > 0 && sum_weight > 0.0 {
        // Distribute leftover to weighted items (flex-grow); unweighted keep base.
        for i in 0..n {
            finals[i] = bases[i];
            if weights[i] > 0.0 {
                finals[i] += (free as f64 * weights[i] / sum_weight).round() as i64;
            }
        }
        // Absorb rounding remainder into the last weighted item so the axis is exact.
        let used: i64 = finals.iter().sum();
        if let Some(pos) = weights.iter().rposition(|&w| w > 0.0) {
            finals[pos] += total - used;
        }
    } else if free > 0 {
        // No weights: measured items keep their natural base. Stretch grows all equally.
        if matches!(justify, Justify::Stretch) {
            let each = free / n as i64;
            let mut rem = free - each * n as i64;
            for i in 0..n {
                finals[i] = bases[i] + each + (if rem > 0 { rem -= 1; 1 } else { 0 });
            }
        } else {
            for i in 0..n {
                finals[i] = bases[i];
            }
        }
    } else if free < 0 {
        // Overflow: shrink. Pure-weight when weights exist (legacy); else proportional
        // to base, each clamped to ≥1 so items never disappear.
        if sum_weight > 0.0 {
            for i in 0..n {
                finals[i] = (total as f64 * weights[i] / sum_weight).round() as i64;
            }
            let used: i64 = finals.iter().sum();
            if let Some(pos) = weights.iter().rposition(|&w| w > 0.0) {
                finals[pos] += total - used;
            }
        } else if sum_base > 0 {
            for i in 0..n {
                finals[i] =
                    ((total as f64 * bases[i] as f64 / sum_base as f64).round() as i64).max(1);
            }
            let used: i64 = finals.iter().sum();
            finals[n - 1] += total - used;
        } else {
            let each = total / n as i64;
            let mut rem = total - each * n as i64;
            for i in 0..n {
                finals[i] = each + (if rem > 0 { rem -= 1; 1 } else { 0 });
            }
        }
    } else {
        // free == 0: bases exactly fill the axis.
        for i in 0..n {
            finals[i] = bases[i];
        }
    }

    // Clamp to a valid u16 range (defensive against sub-pixel drift).
    for f in finals.iter_mut() {
        if *f < 0 {
            *f = 0;
        }
        if *f > total {
            *f = total;
        }
    }

    // --- Position along the main axis by justify, using the final sizes. ---
    let total_size: i64 = finals.iter().sum();
    let sizes = finals;

    let pack_from = |start: i64| -> Vec<i64> {
        let mut acc = start;
        let mut out = vec![0i64; n];
        for i in 0..n {
            out[i] = acc;
            acc += sizes[i];
        }
        out
    };

    let offsets: Vec<i64> = match justify {
        // Start packs from the origin; Stretch already filled the axis.
        Justify::Start | Justify::Stretch => pack_from(0),
        Justify::Center => {
            let gap = (total - total_size).max(0);
            pack_from(gap / 2)
        }
        Justify::End => pack_from((total - total_size).max(0)),
        Justify::SpaceBetween => {
            if n <= 1 {
                pack_from(0)
            } else {
                let gap = (total - total_size).max(0);
                let spacing = gap / (n as i64 - 1);
                let mut out = vec![0i64; n];
                let mut acc = 0;
                for i in 0..n {
                    out[i] = acc;
                    acc += sizes[i] + spacing;
                }
                out
            }
        }
        Justify::SpaceAround => {
            let gap = (total - total_size).max(0);
            let spacing = gap / n as i64;
            pack_from(spacing / 2)
                .iter()
                .enumerate()
                .map(|(i, &o)| o + spacing * i as i64)
                .collect()
        }
        Justify::SpaceEvenly => {
            let gap = (total - total_size).max(0);
            let spacing = gap / (n as i64 + 1);
            pack_from(spacing)
        }
    };

    sizes
        .iter()
        .zip(offsets.iter())
        .map(|(&size, &offset)| {
            make_rect(direction, area, offset.max(0) as u16, size.max(0) as u16)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Equal split used as a fallback.
fn equal_split(direction: Direction, area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return vec![];
    }
    let total = match direction {
        Direction::Horizontal => area.width,
        Direction::Vertical => area.height,
    };
    let each = total / n as u16;
    let mut rects = Vec::with_capacity(n);
    let base = match direction {
        Direction::Horizontal => area.x,
        Direction::Vertical => area.y,
    };
    for i in 0..n {
        let offset = base + (each * i as u16);
        // Last item gets the remainder.
        let size = if i == n - 1 {
            total - each * (n as u16 - 1)
        } else {
            each
        };
        rects.push(make_rect(direction, area, offset - base, size));
    }
    rects
}

fn size_axis(rect: &Rect, direction: Direction) -> u16 {
    match direction {
        Direction::Horizontal => rect.width,
        Direction::Vertical => rect.height,
    }
}

fn size_from_direction(area: Rect, direction: Direction) -> u16 {
    match direction {
        Direction::Horizontal => area.width,
        Direction::Vertical => area.height,
    }
}

fn make_rect(direction: Direction, area: Rect, offset: u16, size: u16) -> Rect {
    match direction {
        Direction::Horizontal => Rect {
            x: area.x + offset,
            y: area.y,
            width: size,
            height: area.height,
        },
        Direction::Vertical => Rect {
            x: area.x,
            y: area.y + offset,
            width: area.width,
            height: size,
        },
    }
}

/// Shift items along the main axis by `start_offset`.
fn shift_items(
    items: &[(Rect, u16)],
    total_area: Rect,
    direction: Direction,
    start_offset: u16,
) -> Vec<Rect> {
    let base = match direction {
        Direction::Horizontal => total_area.x,
        Direction::Vertical => total_area.y,
    };
    let mut result = Vec::with_capacity(items.len());
    let mut pos = base + start_offset;
    for (rect, size) in items {
        result.push(set_offset(*rect, total_area, direction, pos - base));
        pos += size;
    }
    result
}

fn set_offset(rect: Rect, _total_area: Rect, direction: Direction, offset: u16) -> Rect {
    match direction {
        Direction::Horizontal => Rect {
            x: _total_area.x + offset,
            ..rect
        },
        Direction::Vertical => Rect {
            y: _total_area.y + offset,
            ..rect
        },
    }
}

fn set_cross_offset(item: Rect, container: Rect, direction: Direction, offset: u16) -> Rect {
    match direction {
        Direction::Horizontal => Rect {
            y: container.y + offset,
            ..item
        },
        Direction::Vertical => Rect {
            x: container.x + offset,
            ..item
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_area() -> Rect {
        Rect::new(0, 0, 100, 30)
    }

    #[test]
    fn weighted_split_equal_when_no_weights() {
        let area = test_area();
        let result = weighted_split(Direction::Horizontal, area, &[None, None, None]);
        assert_eq!(result.len(), 3);
        // Third item gets remainder: 34, 33, 33 — or similar.
        let total_width: u16 = result.iter().map(|r| r.width).sum();
        assert_eq!(total_width, 100);
    }

    #[test]
    fn weighted_split_respects_weights() {
        let area = test_area();
        let result = weighted_split(Direction::Vertical, area, &[Some(3.0), Some(1.0)]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].height, 22); // 30 * 0.75 = 22.5 -> 22
        assert_eq!(result[1].height, 8); // remainder
        assert_eq!(result[0].height + result[1].height, 30);
    }

    #[test]
    fn weighted_split_mixed_weights() {
        let area = test_area();
        // None = 1.0, Some(2.0) = 2.0 -> total 3.0
        let result = weighted_split(Direction::Horizontal, area, &[None, Some(2.0)]);
        assert_eq!(result.len(), 2);
        let total: u16 = result.iter().map(|r| r.width).sum();
        assert_eq!(total, 100);
        // First should be ~33, second ~66.
        assert!(result[0].width < result[1].width);
    }

    #[test]
    fn weighted_split_empty() {
        let area = test_area();
        let result = weighted_split(Direction::Horizontal, area, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn apply_align_stretch_horizontal() {
        let container = Rect::new(0, 0, 100, 30);
        let item = Rect::new(10, 5, 50, 10);
        let result = apply_align(Align::Stretch, item, container, Direction::Horizontal);
        assert_eq!(result.y, 0);
        assert_eq!(result.height, 30);
        assert_eq!(result.width, 50);
    }

    #[test]
    fn apply_align_center_vertical() {
        let container = Rect::new(0, 0, 100, 30);
        let item = Rect::new(0, 0, 10, 10);
        let result = apply_align(Align::Center, item, container, Direction::Vertical);
        assert_eq!(result.x, 45); // (100 - 10) / 2
    }

    #[test]
    fn apply_justify_space_between() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 20, 30), 20),
            (Rect::new(20, 0, 20, 30), 20),
            (Rect::new(40, 0, 20, 30), 20),
        ];
        let result = apply_justify(Justify::SpaceBetween, &items, container, Direction::Horizontal);
        assert_eq!(result.len(), 3);
        // 100 - 60 = 40 gap, 40 / 2 = 20 spacing
        assert_eq!(result[0].x, 0);
        assert_eq!(result[1].x, 40);
        assert_eq!(result[2].x, 80);
    }

    #[test]
    fn apply_justify_center() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 20, 30), 20),
        ];
        let result = apply_justify(Justify::Center, &items, container, Direction::Horizontal);
        assert_eq!(result[0].x, 40); // (100 - 20) / 2
    }

    #[test]
    fn apply_justify_end_vertical() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 100, 10), 10),
        ];
        let result = apply_justify(Justify::End, &items, container, Direction::Vertical);
        assert_eq!(result[0].y, 20); // 30 - 10
    }

    // --- SpaceAround tests ---

    #[test]
    fn apply_justify_space_around_three_items() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 20, 30), 20),
            (Rect::new(20, 0, 20, 30), 20),
            (Rect::new(40, 0, 20, 30), 20),
        ];
        let result = apply_justify(Justify::SpaceAround, &items, container, Direction::Horizontal);
        assert_eq!(result.len(), 3);
        // 100 - 60 = 40 gap, spacing = 40 / 3 = 13, start_offset = 13 / 2 = 6
        // item0: offset = 6
        // item1: offset = 6 + 20 + 13 = 39
        // item2: offset = 39 + 20 + 13 = 72
        assert_eq!(result[0].x, 6);
        assert_eq!(result[1].x, 39);
        assert_eq!(result[2].x, 72);
    }

    #[test]
    fn apply_justify_space_around_single_item() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 20, 30), 20),
        ];
        let result = apply_justify(Justify::SpaceAround, &items, container, Direction::Horizontal);
        assert_eq!(result.len(), 1);
        // 100 - 20 = 80 gap, spacing = 80 / 1 = 80, start_offset = 40
        assert_eq!(result[0].x, 40);
    }

    #[test]
    fn apply_justify_space_around_empty() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![];
        let result = apply_justify(Justify::SpaceAround, &items, container, Direction::Horizontal);
        assert!(result.is_empty());
    }

    // --- SpaceEvenly tests ---

    #[test]
    fn apply_justify_space_evenly_three_items() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 20, 30), 20),
            (Rect::new(20, 0, 20, 30), 20),
            (Rect::new(40, 0, 20, 30), 20),
        ];
        let result = apply_justify(Justify::SpaceEvenly, &items, container, Direction::Horizontal);
        assert_eq!(result.len(), 3);
        // 100 - 60 = 40 gap, spacing = 40 / 4 = 10
        // item0: offset = 10
        // item1: offset = 10 + 20 + 10 = 40
        // item2: offset = 40 + 20 + 10 = 70
        assert_eq!(result[0].x, 10);
        assert_eq!(result[1].x, 40);
        assert_eq!(result[2].x, 70);
    }

    #[test]
    fn apply_justify_space_evenly_single_item() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 20, 30), 20),
        ];
        let result = apply_justify(Justify::SpaceEvenly, &items, container, Direction::Horizontal);
        assert_eq!(result.len(), 1);
        // 100 - 20 = 80 gap, spacing = 80 / 2 = 40
        assert_eq!(result[0].x, 40);
    }

    #[test]
    fn apply_justify_space_evenly_empty() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![];
        let result = apply_justify(Justify::SpaceEvenly, &items, container, Direction::Horizontal);
        assert!(result.is_empty());
    }

    // --- Stretch tests ---

    #[test]
    fn apply_justify_stretch_three_items() {
        let container = Rect::new(0, 0, 99, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 20, 30), 20),
            (Rect::new(20, 0, 20, 30), 20),
            (Rect::new(40, 0, 20, 30), 20),
        ];
        let result = apply_justify(Justify::Stretch, &items, container, Direction::Horizontal);
        assert_eq!(result.len(), 3);
        // each = 99 / 3 = 33, last gets remainder 99 - 66 = 33
        assert_eq!(result[0].x, 0);
        assert_eq!(result[0].width, 33);
        assert_eq!(result[1].x, 33);
        assert_eq!(result[1].width, 33);
        assert_eq!(result[2].x, 66);
        assert_eq!(result[2].width, 33);
        // Total fills container
        let total: u16 = result.iter().map(|r| r.width).sum();
        assert_eq!(total, 99);
    }

    #[test]
    fn apply_justify_stretch_with_remainder() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 10, 30), 10),
            (Rect::new(10, 0, 10, 30), 10),
            (Rect::new(20, 0, 10, 30), 10),
        ];
        let result = apply_justify(Justify::Stretch, &items, container, Direction::Horizontal);
        assert_eq!(result.len(), 3);
        // each = 100 / 3 = 33, last gets remainder 100 - 66 = 34
        assert_eq!(result[0].width, 33);
        assert_eq!(result[1].width, 33);
        assert_eq!(result[2].width, 34);
        let total: u16 = result.iter().map(|r| r.width).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn apply_justify_stretch_vertical() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![
            (Rect::new(0, 0, 100, 5), 5),
            (Rect::new(0, 5, 100, 5), 5),
        ];
        let result = apply_justify(Justify::Stretch, &items, container, Direction::Vertical);
        assert_eq!(result.len(), 2);
        // each = 30 / 2 = 15
        assert_eq!(result[0].y, 0);
        assert_eq!(result[0].height, 15);
        assert_eq!(result[1].y, 15);
        assert_eq!(result[1].height, 15);
        let total: u16 = result.iter().map(|r| r.height).sum();
        assert_eq!(total, 30);
    }

    #[test]
    fn apply_justify_stretch_empty() {
        let container = Rect::new(0, 0, 100, 30);
        let items: Vec<(Rect, u16)> = vec![];
        let result = apply_justify(Justify::Stretch, &items, container, Direction::Horizontal);
        assert!(result.is_empty());
    }

    // --- flex_layout tests ---

    #[test]
    fn flex_layout_all_none_matches_weighted_split() {
        // Two legacy (None) children in a 30-tall area → equal split, identical to
        // the old weighted_split behavior (zero-regression invariant).
        let area = Rect::new(0, 0, 100, 30);
        let items = vec![(None, None), (None, None)];
        let result = flex_layout(Direction::Vertical, area, &items, Justify::Start);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].height, 15);
        assert_eq!(result[1].height, 15);
        let total: u16 = result.iter().map(|r| r.height).sum();
        assert_eq!(total, 30);
    }

    #[test]
    fn flex_layout_measured_children_pack_to_natural() {
        // Two measured children (natural 3 each), no weights, default Start → each
        // gets exactly 3, packed at the top, rest of the 30-tall area left empty.
        let area = Rect::new(0, 0, 100, 30);
        let items = vec![(Some(3u16), None), (Some(3u16), None)];
        let result = flex_layout(Direction::Vertical, area, &items, Justify::Start);
        assert_eq!(result[0].y, 0);
        assert_eq!(result[0].height, 3);
        assert_eq!(result[1].y, 3);
        assert_eq!(result[1].height, 3);
    }

    #[test]
    fn flex_layout_unmeasured_fills_leftover() {
        // A measured child (3) keeps its natural height; an unmeasured sibling
        // (None) absorbs the leftover 27.
        let area = Rect::new(0, 0, 100, 30);
        let items = vec![(Some(3u16), None), (None, None)];
        let result = flex_layout(Direction::Vertical, area, &items, Justify::Start);
        assert_eq!(result[0].height, 3);
        assert_eq!(result[1].height, 27);
        assert_eq!(result[1].y, 3);
    }

    #[test]
    fn flex_layout_weight_grows_measured_child() {
        // A measured child with an explicit weight grows beyond its natural height.
        let area = Rect::new(0, 0, 100, 30);
        // (natural 3, weight 2.0) + legacy (None) → bases sum 3, free 27.
        // weight 2.0 vs implicit 1.0: measured gets 27 * 2/3 = 18 → 21 total;
        // legacy gets 27 * 1/3 = 9.
        let items = vec![(Some(3u16), Some(2.0)), (None, None)];
        let result = flex_layout(Direction::Vertical, area, &items, Justify::Start);
        assert_eq!(result[0].height, 21);
        assert_eq!(result[1].height, 9);
        let total: u16 = result.iter().map(|r| r.height).sum();
        assert_eq!(total, 30);
    }

    #[test]
    fn flex_layout_stretch_fills_axis() {
        // No weights + Justify::Stretch → all measured children grow equally.
        let area = Rect::new(0, 0, 100, 30);
        let items = vec![(Some(3u16), None), (Some(3u16), None)];
        let result = flex_layout(Direction::Vertical, area, &items, Justify::Stretch);
        assert_eq!(result.len(), 2);
        let total: u16 = result.iter().map(|r| r.height).sum();
        assert_eq!(total, 30);
        assert_eq!(result[0].height, 15);
        assert_eq!(result[1].height, 15);
    }

    #[test]
    fn flex_layout_center_offsets_packed_items() {
        // Two natural-3 items in 30: total 6, gap 24, centered → start at 12.
        let area = Rect::new(0, 0, 100, 30);
        let items = vec![(Some(3u16), None), (Some(3u16), None)];
        let result = flex_layout(Direction::Vertical, area, &items, Justify::Center);
        assert_eq!(result[0].y, 12);
        assert_eq!(result[1].y, 15);
        assert_eq!(result[0].height, 3);
    }

    #[test]
    fn flex_layout_overflow_shrinks_proportionally() {
        // Three natural-3 items (sum 9) in a 5-tall area → shrink proportionally,
        // each at least 1, no panic.
        let area = Rect::new(0, 0, 100, 5);
        let items = vec![(Some(3u16), None), (Some(3u16), None), (Some(3u16), None)];
        let result = flex_layout(Direction::Vertical, area, &items, Justify::Start);
        let total: u16 = result.iter().map(|r| r.height).sum();
        assert_eq!(total, 5);
        assert!(result.iter().all(|r| r.height >= 1));
    }

    #[test]
    fn flex_layout_horizontal_uses_width() {
        // On the horizontal axis, main size = width.
        let area = Rect::new(0, 0, 100, 30);
        let items = vec![(Some(10u16), None), (None, None)];
        let result = flex_layout(Direction::Horizontal, area, &items, Justify::Start);
        assert_eq!(result[0].width, 10);
        assert_eq!(result[1].width, 90);
        assert_eq!(result[1].x, 10);
    }
}
