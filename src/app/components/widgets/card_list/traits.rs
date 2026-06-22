use crate::app::components::SortOrder;
use crate::app::AppEvent;

use super::CardListModel;

/// Trait defining the full contract for a page that displays a card list.
///
/// Extends `CardListModel` (data layer) with page-level concerns: identity,
/// empty state, sort capabilities, and event handling. Analogous to `PageModel`
/// for the details page framework.
pub trait CardListPageModel: CardListModel {
    // Page identity (used for sort persistence)

    fn page_id(&self) -> &str;

    // Empty state

    fn empty_title(&self) -> String;
    fn empty_description(&self) -> String;
    fn empty_icon(&self) -> &str { "emblem-music-symbolic" }

    // Sort capabilities

    fn available_sort_orders(&self) -> &[SortOrder] {
        &[SortOrder::RecentlyAdded, SortOrder::Alphabetic, SortOrder::Creator]
    }

    // Event handling

    /// Returns true if this event means the card list data was updated.
    fn should_refresh(&self, event: &AppEvent) -> bool;
}
