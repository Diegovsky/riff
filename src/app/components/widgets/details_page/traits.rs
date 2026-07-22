use std::rc::Rc;

use crate::app::components::{
    HeaderBarModel, HeaderImageShape, SimpleHeaderBarModel, SimpleHeaderBarModelWrapper,
};
use crate::app::models::ImageSet;
use crate::app::state::PlaybackEvent;
use crate::app::AppEvent;

use super::DetailsPageModel;

/// Provides a default `to_headerbar_model()` implementation for any page model
/// that implements `SimpleHeaderBarModel` and derefs to `DetailsPageModel`.
///
/// This avoids duplicating the same headerbar wiring in every concrete model.
pub trait HasHeaderBarModel:
    SimpleHeaderBarModel + std::ops::Deref<Target = DetailsPageModel> + Sized + 'static
{
    fn to_headerbar_model(self: &Rc<Self>) -> Rc<impl HeaderBarModel + 'static> {
        Rc::new(SimpleHeaderBarModelWrapper::new(
            self.clone(),
            self.app_model.clone(),
            self.dispatcher.box_clone(),
        ))
    }
}

/// Trait defining the contract between a detail page's UI and its model.
///
/// The generic `DetailsPageComponent` uses this trait to wire all standard
/// behavior (header, buttons, events) automatically. Pages only need to
/// implement the methods relevant to them; everything else has sensible defaults.
pub trait PageModel {
    // Page identity

    fn get_title(&self) -> Option<String>;
    fn get_subtitle(&self) -> Option<String> {
        None
    }
    fn get_artwork(&self) -> Option<ImageSet> {
        None
    }
    fn get_caption(&self) -> Option<String> {
        None
    }
    fn header_image_shape(&self) -> HeaderImageShape;
    fn default_icon(&self) -> Option<&str> {
        None
    }

    // Loading

    fn load_page_info(&self) {}
    fn load_more(&self) {}
    fn is_loaded(&self) -> bool {
        false
    }

    // Playback

    fn has_play_button(&self) -> bool {
        false
    }
    fn source_is_playing(&self) -> bool {
        false
    }
    fn start_play(&self, _id: &str) {}
    fn toggle_play(&self) {}
    fn shuffle_play(&self) {}

    // Like/Save

    fn has_like_button(&self) -> bool {
        false
    }
    fn is_liked(&self) -> bool {
        false
    }
    fn toggle_like(&self) {}
    fn like_visible(&self) -> bool {
        self.has_like_button()
    }

    // Info button

    fn has_info_button(&self) -> bool {
        false
    }
    fn on_info_clicked(&self) {}

    // Share button

    fn has_share_button(&self) -> bool {
        false
    }
    fn on_share_clicked(&self) {}

    // Subtitle click

    fn has_subtitle_link(&self) -> bool {
        false
    }
    fn on_subtitle_clicked(&self) {}

    // Event handling

    /// Returns true if this event means page details should be refreshed.
    fn should_refresh_details(&self, event: &AppEvent) -> bool;
    /// Returns true if this event means liked state changed.
    fn should_refresh_liked(&self, _event: &AppEvent) -> bool {
        false
    }
}

/// Check if a playback event means we should update the play button.
/// Returns `Some(true)` for resumed/track changed, `Some(false)` for paused, `None` otherwise.
pub fn is_playback_event(event: &AppEvent) -> Option<bool> {
    match event {
        AppEvent::PlaybackEvent(PlaybackEvent::PlaybackPaused) => Some(false),
        AppEvent::PlaybackEvent(PlaybackEvent::PlaybackResumed)
        | AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_)) => Some(true),
        _ => None,
    }
}
