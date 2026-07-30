// Shared details page framework.
//
// Provides reusable building blocks for album, artist, playlist, and user
// detail views:
//
// - PageModel: trait defining the contract between a page's UI and model.
// - DetailsPageModel: base struct with shared state (composed via Deref).
// - DetailsPageComponent: generic component that wires standard behavior
//   from a PageModel implementation.
// - DetailsPage: scroll-based layout widget with a collapsing header.
// - DetailsHeader: header widget with artwork, titles, and action buttons.

mod component;
mod header;
mod model;
mod subtitle_links;
mod traits;
mod widget;

pub use component::*;
pub use header::*;
pub use model::*;
pub use subtitle_links::*;
pub use traits::*;
pub use widget::*;

/// Size (in pixels) used to fetch and display the header artwork.
pub(super) const HEADER_IMAGE_SIZE: i32 = 200;

/// Register GObject widget types for this module (called at app startup).
pub fn expose_widgets() {
    header::expose_widgets();
    subtitle_links::expose_widgets();
}
