//! Virtual viewport geometry. The shell reports the window size; these constants are
//! the single source for both rendering math and the CSS layout variables.

use std::ops::Range;

pub const SIDEBAR: f64 = 212.0;
pub const PREVIEW: f64 = 300.0;
pub const PAD_X: f64 = 36.0;
pub const TOP: f64 = 68.0;
pub const BOTTOM: f64 = 20.0;
pub const CELL_W: f64 = 152.0;
pub const CELL_H: f64 = 164.0;
pub const ROW_H: f64 = 30.0;
/// Fraction of the content width used by the Miller parent and preview columns.
pub const PARENT_SHARE: f64 = 0.22;
pub const PREVIEW_SHARE: f64 = 0.38;
/// Grid zoom steps, as multiples of the default cell size.
pub const ZOOM_STEPS: &[f64] = &[
    0.6, 0.75, 0.9, 1.0, 1.25, 1.5, 1.85, 2.25, 2.75, 3.4, 4.0, 4.5,
];

/// The next zoom step in `direction` (+1 larger, -1 smaller, 0 reset), clamped at the ends.
pub fn zoom_step(zoom: f64, direction: i32) -> f64 {
    let index = ZOOM_STEPS
        .iter()
        .position(|step| (step - zoom).abs() < 1e-6)
        .unwrap_or(3);
    let next = match direction {
        0 => 3,
        d if d > 0 => (index + 1).min(ZOOM_STEPS.len() - 1),
        _ => index.saturating_sub(1),
    };
    ZOOM_STEPS[next]
}

/// Rows kept between the cursor and the viewport edge, like ranger's scroll offset.
const MARGIN_ROWS: f64 = 1.0;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum View {
    #[default]
    Grid,
    List,
    Columns,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Window {
    pub width: f64,
    pub height: f64,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            width: 1120.0,
            height: 740.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Layout {
    pub columns: usize,
    pub row: f64,
    pub height: f64,
    /// Left edge and width of the scrolling pane, for routing wheel input.
    pub left: f64,
    pub width: f64,
}

impl Layout {
    pub fn new(window: Window, view: View, sidebar: bool, preview: bool, zoom: f64) -> Self {
        let sidebar = if sidebar { SIDEBAR } else { 0.0 };
        let preview = if preview && view != View::Columns {
            PREVIEW
        } else {
            0.0
        };
        let cell = (CELL_W * zoom, CELL_H * zoom);
        let content = (window.width - sidebar - preview - 2.0 * PAD_X).max(ROW_H);
        let height = (window.height - TOP - BOTTOM).max(ROW_H);
        let (left, width) = match view {
            View::Columns => {
                let parent = (content * PARENT_SHARE).floor();
                let current = content - parent - (content * PREVIEW_SHARE).floor();
                (sidebar + PAD_X + parent, current)
            }
            _ => (sidebar + PAD_X, content),
        };
        let (columns, row) = match view {
            View::Grid => (((content / cell.0).floor() as usize).max(1), cell.1),
            _ => (1, ROW_H),
        };
        Self {
            columns,
            row,
            height,
            left,
            width,
        }
    }

    fn rows(&self, count: usize) -> usize {
        count.div_ceil(self.columns)
    }

    pub fn max_scroll(&self, count: usize) -> f64 {
        (self.rows(count) as f64 * self.row - self.height).max(0.0)
    }

    pub fn clamp(&self, scroll: f64, count: usize) -> f64 {
        scroll.clamp(0.0, self.max_scroll(count))
    }

    /// Item indices to render: every row that intersects the viewport.
    pub fn visible(&self, scroll: f64, count: usize) -> Range<usize> {
        let first = (scroll / self.row).floor().max(0.0) as usize;
        let last = ((scroll + self.height) / self.row).ceil() as usize;
        (first * self.columns).min(count)..(last * self.columns).min(count)
    }

    /// Items moved by a half-page or full-page jump.
    pub fn page(&self, fraction: f64) -> usize {
        ((self.height / self.row * fraction).floor() as usize).max(1) * self.columns
    }

    /// The nearest scroll offset that shows `index` with a margin row around it.
    pub fn follow(&self, scroll: f64, index: usize, count: usize) -> f64 {
        let top = (index / self.columns) as f64 * self.row;
        let margin = (MARGIN_ROWS * self.row).min(((self.height - self.row) / 2.0).max(0.0));
        let scroll = if top - margin < scroll {
            top - margin
        } else if top + self.row + margin > scroll + self.height {
            top + self.row + margin - self.height
        } else {
            scroll
        };
        self.clamp(scroll, count)
    }

    /// The item under a window point, using the same tracks the grid renders.
    pub fn index_at(&self, x: f64, y: f64, scroll: f64, count: usize) -> Option<usize> {
        if !self.contains(x, y) {
            return None;
        }
        let track = (self.width / self.columns as f64).floor();
        let column = ((x - self.left) / track) as usize;
        let row = ((y - TOP + scroll) / self.row) as usize;
        let index = row * self.columns + column;
        (column < self.columns && index < count).then_some(index)
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left && x <= self.left + self.width && y >= TOP
    }
}

/// One frame of eased scrolling toward `target`; lands exactly within half a pixel.
pub fn step(current: f64, target: f64, motion: bool) -> f64 {
    let next = current + (target - current) * 0.32;
    if !motion || (target - next).abs() < 0.5 {
        target
    } else {
        next
    }
}

/// CSS custom properties generated from the same constants as the math above.
pub fn css_variables() -> String {
    format!(
        ":root{{--sidebar-w:{SIDEBAR}px;--preview-w:{PREVIEW}px;--pad-x:{PAD_X}px;--top-h:{TOP}px;\
         --bottom-h:{BOTTOM}px;--cell-w:{CELL_W}px;--cell-h:{CELL_H}px;--row-h:{ROW_H}px;\
         --parent-share:{}%;--preview-share:{}%;}}",
        PARENT_SHARE * 100.0,
        PREVIEW_SHARE * 100.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Layout {
        Layout::new(Window::default(), View::Grid, true, false, 1.0)
    }

    #[test]
    fn grid_columns_fill_the_content_width() {
        let layout = grid();
        let content = 1120.0 - SIDEBAR - 2.0 * PAD_X;
        assert_eq!(layout.columns, (content / CELL_W) as usize);
        let wide = Layout::new(Window::default(), View::Grid, false, false, 1.0);
        assert!(wide.columns > layout.columns);
        let list = Layout::new(Window::default(), View::List, true, true, 1.0);
        assert_eq!((list.columns, list.row), (1, ROW_H));
    }

    #[test]
    fn visible_range_covers_partial_rows_only() {
        let layout = grid();
        let range = layout.visible(0.0, 1000);
        let rows = (layout.height / CELL_H).ceil() as usize;
        assert_eq!(range, 0..rows * layout.columns);
        let scrolled = layout.visible(CELL_H * 2.5, 1000);
        assert_eq!(scrolled.start, 2 * layout.columns);
        assert_eq!(layout.visible(0.0, 3), 0..3);
    }

    #[test]
    fn follow_keeps_the_cursor_inside_with_margin() {
        let list = Layout::new(Window::default(), View::List, true, false, 1.0);
        assert_eq!(list.follow(0.0, 3, 1000), 0.0);
        let below = list.follow(0.0, 100, 1000);
        assert_eq!(below, 101.0 * ROW_H + ROW_H - list.height);
        let above = list.follow(below, 10, 1000);
        assert_eq!(above, 9.0 * ROW_H);
        assert_eq!(list.follow(0.0, 999, 1000), list.max_scroll(1000));
        assert_eq!(list.follow(500.0, 0, 5), 0.0, "short folders never scroll");
    }

    #[test]
    fn points_map_to_the_rendered_items() {
        let layout = grid();
        let track = (layout.width / layout.columns as f64).floor();
        let (x, y) = (layout.left + 1.0, TOP + 1.0);
        assert_eq!(layout.index_at(x, y, 0.0, 100), Some(0));
        assert_eq!(layout.index_at(x + track, y, 0.0, 100), Some(1));
        assert_eq!(
            layout.index_at(x, y + CELL_H, 0.0, 100),
            Some(layout.columns)
        );
        assert_eq!(layout.index_at(x, y, CELL_H, 100), Some(layout.columns));
        assert_eq!(layout.index_at(x + track, y, 0.0, 1), None);
        assert_eq!(layout.index_at(10.0, y, 0.0, 100), None, "sidebar");
    }

    #[test]
    fn zoom_steps_clamp_and_reset() {
        assert_eq!(zoom_step(1.0, 1), 1.25);
        assert_eq!(zoom_step(1.0, -1), 0.9);
        assert_eq!(zoom_step(2.25, 1), 2.75);
        assert_eq!(zoom_step(4.5, 1), 4.5);
        assert_eq!(zoom_step(0.6, -1), 0.6);
        assert_eq!(zoom_step(1.85, 0), 1.0);
        let small = Layout::new(Window::default(), View::Grid, true, false, 0.6);
        let large = Layout::new(Window::default(), View::Grid, true, false, 2.25);
        assert!(small.columns > grid().columns && large.columns < grid().columns);
        assert_eq!(large.row, CELL_H * 2.25);
        let maximum = Layout::new(
            Window {
                width: 560.0,
                height: 360.0,
            },
            View::Grid,
            true,
            false,
            4.5,
        );
        assert_eq!(maximum.columns, 1);
        assert!(maximum.left + maximum.width <= 560.0);
        assert_eq!(maximum.row, CELL_H * 4.5);
    }

    #[test]
    fn eased_scrolling_converges_and_respects_reduced_motion() {
        let mut position = 0.0;
        for _ in 0..40 {
            position = step(position, 300.0, true);
        }
        assert_eq!(position, 300.0);
        assert!(step(0.0, 300.0, true) < 300.0);
        assert_eq!(step(0.0, 300.0, false), 300.0);
    }

    #[test]
    fn page_jumps_scale_with_viewport() {
        let list = Layout::new(Window::default(), View::List, true, false, 1.0);
        let full = (list.height / ROW_H).floor() as usize;
        assert_eq!(list.page(1.0), full);
        assert_eq!(list.page(0.5), (list.height / ROW_H * 0.5).floor() as usize);
    }
}
