// Model for the saved tracks (liked songs) page.
// Implements PageModel, TrackListModel, and SimpleHeaderBarModel to drive
// track listing, pagination, playback, and selection.

use gettextrs::gettext;
use std::ops::Deref;
use std::rc::Rc;

use crate::{impl_source_page, impl_source_track_list, impl_track_list_model_base};

use crate::app::components::DetailsPageModel;
use crate::app::components::{
    dispatch_api_read, HasHeaderBarModel, HeaderImageShape, PageModel, PinnedPageModel,
    SimpleHeaderBarModel, TrackListModel,
};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{SelectionAction, SelectionState};
use crate::app::{
    AppEvent, AppModel, BrowserAction, BrowserEvent, Dispatcher, PaginationTarget, SongsSource,
};
use crate::feature_flags::{self, FeatureFlag};

/// Data model for the saved tracks page. Composes `DetailsPageModel` via Deref.
pub struct SavedTracksModel {
    base: DetailsPageModel,
}

impl Deref for SavedTracksModel {
    type Target = DetailsPageModel;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl HasHeaderBarModel for SavedTracksModel {}

impl PinnedPageModel for SavedTracksModel {}

impl SavedTracksModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            base: DetailsPageModel::new_without_id(app_model, dispatcher),
        }
    }

    /// Called on login to load the initial batch of saved tracks.
    pub fn load_initial(&self) {
        let api = self.app_model.api();
        dispatch_api_read(&self.dispatcher, move |tag| async move {
            api.get_saved_tracks(0, 50, tag)
                .await
                .map(|song_batch| BrowserAction::SetSavedTracks(Box::new(song_batch)).into())
        });
    }
}

impl PageModel for SavedTracksModel {
    fn get_title(&self) -> Option<String> {
        Some(gettext("All Tracks"))
    }

    fn get_subtitle(&self) -> Option<String> {
        let loaded = TrackListModel::song_list_model(self).len();
        let count = self
            .app_model
            .get_state()
            .browser
            .home_state()
            .and_then(|s| s.saved_tracks_total)
            .map_or(loaded, |total| total.max(loaded));
        Some(gettextrs::ngettext!(
            "{} Track",
            "{} Tracks",
            count as u32,
            count
        ))
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Square
    }

    fn songs_source(&self) -> Option<SongsSource> {
        Some(SongsSource::SavedTracks)
    }

    fn context_name(&self) -> Option<String> {
        Some(crate::app::components::labels::SAVED_TRACKS.clone())
    }

    fn default_icon(&self) -> Option<&str> {
        Some("emote-love-symbolic")
    }

    fn load_more(&self) {
        let api = self.app_model.api();
        let state = self.app_model.get_state();
        let Some(next_page) = state
            .browser
            .home_state()
            .map(|s| s.next_saved_tracks_page.clone())
        else {
            return;
        };
        drop(state);

        let Some(offset) = next_page.next_offset else {
            return;
        };
        let batch_size = next_page.batch_size;

        self.app_model
            .update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SavedTracks).into());

        dispatch_api_read(&self.dispatcher, move |tag| async move {
            api.get_saved_tracks(offset, batch_size, tag)
                .await
                .map(|song_batch| BrowserAction::AppendSavedTracks(Box::new(song_batch)).into())
        });
    }

    fn is_loaded(&self) -> bool {
        true
    }

    impl_source_page!();

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated)
        )
    }
}

impl TrackListModel for SavedTracksModel {
    fn song_list_model(&self) -> SongListModel {
        self.app_model
            .get_state()
            .browser
            .home_state()
            .expect("illegal attempt to read home_state")
            .saved_tracks
            .clone()
    }

    fn autoscroll_to_playing(&self) -> bool {
        true
    }

    fn show_album_column(&self) -> bool {
        true
    }

    impl_track_list_model_base!();
    impl_source_track_list!();

    fn enable_selection(&self) -> bool {
        self.enable_selection_with_context(SelectionContext::SavedTracks)
    }
}

impl SimpleHeaderBarModel for SavedTracksModel {
    fn selection_context(&self) -> Option<SelectionContext> {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        Some(SelectionContext::SavedTracks)
    }

    fn select_all(&self) {
        let songs: Vec<Track> = TrackListModel::song_list_model(self).collect();
        self.dispatcher
            .dispatch(SelectionAction::Select(songs).into());
    }
}

impl crate::app::ProvidesApi for SavedTracksModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
