mod widget;
pub use widget::*;

mod history_menu_row;
use history_menu_row::HistoryMenuRow;

mod component;
pub use component::*;

mod registrar;
pub use registrar::*;

use glib::prelude::*;

pub fn expose_widgets() {
    widget::AppHeaderBar::static_type();
}
