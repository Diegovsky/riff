//! Shared enums for card display: sort order, card size, and card layout.
//!
//! These live in the models layer because they are used by settings persistence,
//! app state, and UI components alike.

use gettextrs::gettext;
use int_enum::IntEnum;
use std::convert::TryFrom;

/// Controls the sort order for card list pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntEnum)]
#[repr(u8)]
pub enum SortOrder {
    RecentlyAdded = 0,
    Alphabetic = 1,
    Creator = 2,
    DateReleased = 3,
    Popularity = 4,
}

impl SortOrder {
    const COUNT: u8 = 5;

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

    /// Cycle to the next sort order variant, wrapping around.
    pub fn next(self) -> Self {
        Self::try_from((u8::from(self) + 1) % Self::COUNT).unwrap()
    }
}

/// Controls the card image size (small, medium, large).
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntEnum)]
#[repr(u8)]
pub enum CardSize {
    Small = 0,
    Medium = 1,
    Large = 2,
}

impl CardSize {
    const COUNT: u8 = 3;

    pub fn increase(self) -> Self {
        Self::try_from((u8::from(self) + 1).min(Self::COUNT - 1)).unwrap()
    }

    pub fn decrease(self) -> Self {
        Self::try_from(u8::from(self).saturating_sub(1)).unwrap()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntEnum)]
#[repr(u8)]
pub enum CardLayout {
    /// Image on top, title and subtitle below (default grid view).
    Vertical = 0,
    /// Image only, title shown as tooltip (compact grid view).
    ImageOnly = 1,
    /// Image on left, title and subtitle on the right (list view).
    Horizontal = 2,
}

impl CardLayout {
    const COUNT: u8 = 3;

    /// Cycle to the next layout variant.
    pub fn next(self) -> Self {
        Self::try_from((u8::from(self) + 1) % Self::COUNT).unwrap()
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
