mod component;
mod easter_eggs;
mod playback_controls;
mod playback_info;
mod playback_widget;
mod queue_bar;
pub use component::*;
pub use queue_bar::{BarTrack, QueueBarWidget};

use glib::prelude::*;

pub fn expose_widgets() {
    playback_widget::PlaybackWidget::static_type();
    queue_bar::QueueBarWidget::static_type();
}
