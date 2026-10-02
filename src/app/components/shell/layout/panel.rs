//! A side panel: the sidebar of an `Adw.OverlaySplitView`, resized by dragging.

use std::cell::Cell;

use super::PANEL_COLLAPSE_SLACK;

#[derive(Clone, Copy, Debug)]
pub struct PanelSize {
    pub min_width: i32,
    pub max_width: i32,
    pub max_fraction: f64,
    pub default_width: i32,
}

impl PanelSize {
    pub fn shown_width(&self, width: i32, window_width: i32) -> i32 {
        width
            .min((f64::from(window_width) * self.max_fraction) as i32)
            .max(self.min_width)
    }

    pub fn resized_width(&self, width: i32) -> Option<i32> {
        (width >= self.min_width - PANEL_COLLAPSE_SLACK)
            .then(|| width.clamp(self.min_width, self.max_width))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

pub struct Panel {
    pub size: PanelSize,
    pub split: libadwaita::OverlaySplitView,
    edge: Edge,
    width: Cell<i32>,
}

impl Panel {
    pub fn new(size: PanelSize, split: libadwaita::OverlaySplitView, edge: Edge) -> Self {
        Self {
            size,
            split,
            edge,
            width: Cell::new(size.default_width),
        }
    }

    pub fn width(&self) -> i32 {
        self.width.get()
    }

    pub fn set_width(&self, width: i32) {
        self.width.set(width);
    }

    pub fn shown_width(&self, window_width: i32) -> i32 {
        self.size.shown_width(self.width.get(), window_width)
    }

    pub fn apply(&self, window_width: i32) {
        let split = &self.split;
        let (min, max) = if split.is_collapsed() {
            (0.0, f64::from(i32::MAX))
        } else {
            let width = f64::from(self.shown_width(window_width));
            (width, width)
        };
        if split.sidebar_width_fraction() != 1.0 {
            split.set_sidebar_width_fraction(1.0);
        }
        if split.min_sidebar_width() != min || split.max_sidebar_width() != max {
            split.set_min_sidebar_width(min);
            split.set_max_sidebar_width(max);
        }
    }

    pub fn dragged_width(&self, start_width: i32, offset_x: f64) -> i32 {
        match self.edge {
            Edge::Start => start_width + offset_x as i32,
            Edge::End => start_width - offset_x as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widths() {
        let size = PanelSize {
            min_width: 280,
            max_width: 600,
            max_fraction: 1.0 / 3.0,
            default_width: 360,
        };
        assert_eq!(size.resized_width(400), Some(400));
        assert_eq!(size.resized_width(700), Some(600));
        assert_eq!(size.resized_width(279), Some(280));
        assert_eq!(size.resized_width(280 - PANEL_COLLAPSE_SLACK), Some(280));
        assert_eq!(size.resized_width(280 - PANEL_COLLAPSE_SLACK - 1), None);

        assert_eq!(size.shown_width(400, 1500), 400);
        assert_eq!(size.shown_width(400, 900), 300);
        assert_eq!(size.shown_width(400, 600), 280);
        assert_eq!(size.shown_width(600, 3000), 600);
    }
}
