//! Layout calculation for Row / Column containers.
//!
//! Provides flex-grow–style weighted splitting, justify (main-axis), and
//! align (cross-axis) helpers that work on [`ratatui::layout::Rect`].

use ratatui::layout::{Direction, Rect};

use crate::core::protocol::common_types::{Align, Justify};

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
}
