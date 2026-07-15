// Model for the saved tracks (liked songs) page.
// Implements PageModel, PlaylistModel, and SimpleHeaderBarModel to drive
// track listing, pagination, playback, and selection.

use gettextrs::gettext;
use gio::prelude::*;
use gio::SimpleActionGroup;
use std::ops::Deref;
use std::rc::Rc;

use crate::{impl_playlist_model_base, impl_toggle_play};

use crate::app::components::DetailsPageModel;
use crate::app::components::{
    labels, HasHeaderBarModel, HeaderImageShape, PageModel, PlaylistModel, SimpleHeaderBarModel,
};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{PlaybackAction, SelectionAction, SelectionState};
use crate::app::{
    ActionDispatcher, AppEvent, AppModel, BatchQuery, BrowserAction, BrowserEvent,
    PaginationTarget, SongsSource,
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

impl SavedTracksModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            base: DetailsPageModel::new_without_id(app_model, dispatcher),
        }
    }

    /// Called on login to load the initial batch of saved tracks.
    pub fn load_initial(&self) {
        let loader = self.app_model.get_batch_loader();
        let query = BatchQuery {
            source: SongsSource::SavedTracks,
            batch: Batch::first_of_size(50),
        };
        self.dispatcher.dispatch_async(Box::pin(async move {
            loader
                .query(query, |_s, song_batch| {
                    BrowserAction::SetSavedTracks(Box::new(song_batch)).into()
                })
                .await
        }));
    }
}

impl PageModel for SavedTracksModel {
    fn get_title(&self) -> Option<String> {
        Some(gettext("All Tracks"))
    }

    fn get_subtitle(&self) -> Option<String> {
        let count = PlaylistModel::song_list_model(self).len();
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

    fn default_icon(&self) -> Option<&str> {
        Some("emote-love-symbolic")
    }

    fn load_more(&self) {
        let api = self.app_model.get_spotify();
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

        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.get_saved_tracks(offset, batch_size)
                    .await
                    .map(|song_batch| BrowserAction::AppendSavedTracks(Box::new(song_batch)).into())
            });
    }

    fn is_loaded(&self) -> bool {
        true
    }

    fn has_play_button(&self) -> bool {
        true
    }

    fn source_is_playing(&self) -> bool {
        matches!(
            self.app_model.get_state().playback.current_source(),
            Some(SongsSource::SavedTracks)
        )
    }

    impl_toggle_play!();

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated)
        )
    }
}

impl PlaylistModel for SavedTracksModel {
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

    impl_playlist_model_base!();

    fn enable_selection(&self) -> bool {
        self.enable_selection_with_context(SelectionContext::SavedTracks)
    }

    fn play_song_at(&self, pos: usize, id: &str) {
        let batch = PlaylistModel::song_list_model(self).song_batch_for(pos);
        if let Some(batch) = batch {
            self.dispatcher
                .dispatch(PlaybackAction::LoadPagedSongs(SongsSource::SavedTracks, batch).into());
            self.dispatcher
                .dispatch(PlaybackAction::Load(id.to_string()).into());
        }
    }

    fn actions_for(&self, song: &SongDescription) -> Option<gio::ActionGroup> {
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.box_clone(), None) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.box_clone(), None));
        group.add_action(&song.make_link_action(None));
        Some(group.upcast())
    }

    fn menu_for(&self, song: &SongDescription) -> Option<gio::MenuModel> {
        let menu = gio::Menu::new();
        menu.append(Some(&*labels::VIEW_ALBUM), Some("song.view_album"));
        for artist in song.artists.iter() {
            menu.append(
                Some(&labels::more_from_label(&artist.name)),
                Some(&format!("song.view_artist_{}", artist.id)),
            );
        }
        menu.append(Some(&*labels::COPY_LINK), Some("song.copy_link"));
        Some(menu.upcast())
    }
}

impl SimpleHeaderBarModel for SavedTracksModel {
    fn title(&self) -> Option<String> {
        Some(gettext("All Tracks"))
    }
    fn title_updated(&self, _: &AppEvent) -> bool {
        false
    }

    fn selection_context(&self) -> Option<SelectionContext> {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        Some(SelectionContext::SavedTracks)
    }

    fn select_all(&self) {
        let songs: Vec<SongDescription> = PlaylistModel::song_list_model(self).collect();
        self.dispatcher
            .dispatch(SelectionAction::Select(songs).into());
    }
}
