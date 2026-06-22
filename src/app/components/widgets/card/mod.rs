//! Card widget module.
//!
//! Provides `CardWidget`, a reusable artwork tile used in grid/list views
//! for albums, artists, and playlists. The `ImageShape` enum controls whether
//! artwork is circular or square. Layout/size/sort enums live in `app::models`.

mod widget;
pub use widget::{CardWidget, ImageShape, IMAGE_SIZE};
