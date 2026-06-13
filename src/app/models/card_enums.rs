//! Shared enums for card display: sort order, card size, and card layout.
//!
//! These live in the models layer because they are used by settings persistence,
//! app state, and UI components alike.

use gettextrs::gettext;

/// Controls the sort order for card list pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    RecentlyAdded,
    Alphabetic,
    Creator,
    DateReleased,
    Popularity,
}

impl SortOrder {
    /// GSettings-compatible string key for this sort order.
    pub fn to_str(self) -> &'static str {
        match self {
            Self::RecentlyAdded => "recently-added",
            Self::Alphabetic => "alphabetic",
            Self::Creator => "creator",
            Self::DateReleased => "date-released",
            Self::Popularity => "popularity",
        }
    }

    /// Parse from a GSettings string key, defaulting to `RecentlyAdded`.
    pub fn parse_key(s: &str) -> Self {
        match s {
            "alphabetic" => Self::Alphabetic,
            "creator" => Self::Creator,
            "date-released" => Self::DateReleased,
            "popularity" => Self::Popularity,
            _ => Self::RecentlyAdded,
        }
    }

    /// Translatable user-facing label for display in menus.
    pub fn label(self) -> String {
        match self {
            // Translators: Sort option — show items in the order they were saved
            Self::RecentlyAdded => gettext("Recently Added"),
            // Translators: Sort option — alphabetical order by title
            Self::Alphabetic => gettext("Alphabetic"),
            // Translators: Sort option — group by artist/creator name
            Self::Creator => gettext("Creator"),
            // Translators: Sort option — order by release date (newest first)
            Self::DateReleased => gettext("Date Released"),
            // Translators: Sort option — order by popularity score (highest first)
            Self::Popularity => gettext("Popularity"),
        }
    }
}

/// Controls the card image size (small, medium, large).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardSize {
    Small,
    Medium,
    Large,
}

impl CardSize {
    pub fn increase(self) -> Self {
        match self {
            Self::Small => Self::Medium,
            Self::Medium => Self::Large,
            Self::Large => Self::Large,
        }
    }

    pub fn decrease(self) -> Self {
        match self {
            Self::Small => Self::Small,
            Self::Medium => Self::Small,
            Self::Large => Self::Medium,
        }
    }

    /// Returns the pixel dimension for this size variant.
    pub fn pixel_size(self) -> i32 {
        match self {
            Self::Small => 100,
            Self::Medium => 140,
            Self::Large => 180,
        }
    }

    /// CSS class applied to the widget for this size.
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Small => "card--small",
            Self::Medium => "card--medium",
            Self::Large => "card--large",
        }
    }
}

/// Controls the card's visual layout orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardLayout {
    /// Image on top, title and subtitle below (default grid view).
    Vertical,
    /// Image only, title shown as tooltip (compact grid view).
    ImageOnly,
    /// Image on left, title and subtitle on the right (list view).
    Horizontal,
}

impl CardLayout {
    /// Cycle to the next layout variant.
    pub fn next(self) -> Self {
        match self {
            Self::Vertical => Self::ImageOnly,
            Self::ImageOnly => Self::Horizontal,
            Self::Horizontal => Self::Vertical,
        }
    }

    /// CSS class applied to the widget for this layout.
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Vertical => "card--vertical",
            Self::ImageOnly => "card--image-only",
            Self::Horizontal => "card--horizontal",
        }
    }
}
