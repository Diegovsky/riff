use super::PanelSize;

/// Under this window width, the queue sheet's bottom bar shows.
pub const NARROW_WIDTH_SP: f64 = 764.0;
/// Under this window width, the side panels fill the window when open.
pub const MOBILE_WIDTH_SP: f64 = 550.0;

/// The least the content panel keeps beside the side panels.
pub const CONTENT_PANEL_MIN_WIDTH_SP: f64 = 350.0;

/// How far under its minimum width a drag closes a panel.
pub const PANEL_COLLAPSE_SLACK: i32 = 90;

pub const QUEUE_SHEET_MAX_WIDTH: i32 = 1020;

pub const NAVIGATION_PANEL: PanelSize = PanelSize {
    min_width: 212,
    max_width: 400,
    max_fraction: 1.0 / 3.0,
    default_width: 280,
};

pub const UTILITY_PANEL: PanelSize = PanelSize {
    min_width: 280,
    max_width: 600,
    max_fraction: 1.0 / 3.0,
    default_width: 360,
};
