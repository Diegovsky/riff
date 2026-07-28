mod widget;
pub use widget::*;

mod component;
pub use component::*;

mod registrar;
pub use registrar::*;

use glib::prelude::*;

pub fn expose_widgets() {
    widget::AppHeaderBar::static_type();
}
