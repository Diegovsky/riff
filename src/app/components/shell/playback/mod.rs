mod component;
mod playback_controls;
mod playback_info;
mod playback_info_mobile;
mod playback_widget;
pub use component::*;
pub use playback_info_mobile::PlaybackInfoMobileWidget;

use glib::prelude::*;

pub fn expose_widgets() {
    playback_widget::PlaybackWidget::static_type();
    playback_info_mobile::PlaybackInfoMobileWidget::static_type();
}
