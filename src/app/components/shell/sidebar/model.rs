use gettextrs::gettext;
use gtk::prelude::*;
use std::rc::Rc;

use super::{SidebarDestination, SidebarItem, PINNED_SECTION, SAVED_PLAYLISTS_SECTION};
use crate::app::components::{
    dispatch_api_call, dispatch_api_call_many, dispatch_api_read, dispatch_api_read_many,
};
use crate::app::models::{CardModel, PlaylistSummary};
use crate::app::state::{PlaybackAction, ScreenName};
use crate::app::{AppAction, AppModel, BrowserAction, Dispatcher, PaginationTarget, SongsSource};
use crate::feature_flags::{is_enabled, FeatureFlag};
use crate::settings;

/// Load `tracks` into the queue via `load`, then start playing from the
/// first track, or from a random one when `shuffle` is set.
fn playback_actions(load: AppAction, track_ids: &[String], shuffle: bool) -> Vec<AppAction> {
    let start = if shuffle && !track_ids.is_empty() {
        track_ids.get(rand::random::<usize>() % track_ids.len())
    } else {
        track_ids.first()
    };
    let mut actions = vec![PlaybackAction::SetShuffled(shuffle).into(), load];
    if let Some(id) = start {
        actions.push(PlaybackAction::Load(id.clone()).into());
    }
    actions
}

pub struct SidebarModel {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
}

impl SidebarModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    fn get_playlists(&self) -> Vec<SidebarDestination> {
        self.app_model
            .get_state()
            .browser
            .home_state()
            .expect("expected HomeState to be available")
            .playlists
            .iter()
            .map(Self::map_to_destination)
            .collect()
    }

    pub fn load_more_playlists(&self) -> Option<()> {
        let api = self.app_model.api();
        let state = self.app_model.get_state();
        let home = state.browser.home_state()?;
        let batch_size = home.next_playlists_page.batch_size;
        let offset = home.next_playlists_page.next_offset?;
        drop(state);

        self.app_model
            .update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SavedPlaylists).into());

        dispatch_api_read(&self.dispatcher, move |tag| async move {
            api.get_saved_playlists(offset, batch_size, tag)
                .await
                .map(|page| BrowserAction::AppendPlaylistsContent(page.items).into())
        });

        Some(())
    }

    fn map_to_destination(a: CardModel) -> SidebarDestination {
        let title = Some(a.title())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| gettext("Unnamed Playlist"));
        let id = a.id();
        SidebarDestination::Playlist(PlaylistSummary { id, title })
    }

    pub(super) fn create_new_playlist(&self, name: String) {
        let user_id = self.app_model.get_state().logged_user.user.clone().unwrap();
        let api = self.app_model.api();
        dispatch_api_call(&self.dispatcher, move || async move {
            api.create_playlist(user_id.as_str(), name.as_str())
                .await
                .map(AppAction::CreatePlaylist)
        })
    }

    pub(super) fn is_playlist_owned(&self, id: &str) -> bool {
        self.app_model
            .get_state()
            .logged_user
            .playlist_ids
            .contains(id)
    }

    pub(super) fn logged_user_id(&self) -> Option<String> {
        self.app_model.get_state().logged_user.user.clone()
    }

    pub(super) fn unfollow_playlist(&self, id: String) {
        let pin_enabled = is_enabled(FeatureFlag::PinnedObjects);
        let user_id = self.logged_user_id();
        let api = self.app_model.api();
        dispatch_api_call(&self.dispatcher, move || async move {
            api.unfollow_playlist(&id).await?;
            if pin_enabled {
                if let Some(user_id) = user_id {
                    settings::unpin_object(&user_id, settings::PinnedKind::Playlist, &id);
                }
            }
            Ok(AppAction::RemovePlaylist(id))
        })
    }

    pub(super) fn play_playlist(&self, id: String) {
        let api = self.app_model.api();
        let source = SongsSource::Playlist(id.clone());
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let batch = api.get_playlist_tracks(&id, 0, 50, tag).await?;
            let first_id = batch.items.first().map(|s| s.rri.id.clone());
            let mut actions: Vec<AppAction> = vec![
                PlaybackAction::SetShuffled(false).into(),
                PlaybackAction::LoadPagedSongs(source, batch).into(),
            ];
            if let Some(track_id) = first_id {
                actions.push(PlaybackAction::Load(track_id).into());
            }
            Ok(actions)
        });
    }

    pub(super) fn shuffle_playlist(&self, id: String) {
        let api = self.app_model.api();
        let source = SongsSource::Playlist(id.clone());
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let batch = api.get_playlist_tracks(&id, 0, 50, tag).await?;
            let len = batch.items.len();
            let track_id = if len > 0 {
                let index = rand::random::<usize>() % len;
                Some(batch.items[index].rri.id.clone())
            } else {
                None
            };
            let mut actions: Vec<AppAction> = vec![
                PlaybackAction::SetShuffled(true).into(),
                PlaybackAction::LoadPagedSongs(source, batch).into(),
            ];
            if let Some(track_id) = track_id {
                actions.push(PlaybackAction::Load(track_id).into());
            }
            Ok(actions)
        });
    }

    /// Play a pinned album from its first track, or shuffled.
    pub(super) fn play_album(&self, id: String, shuffle: bool) {
        let api = self.app_model.api();
        let source = SongsSource::Album(id.clone());
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let batch = api.get_album_tracks(&id, 0, 50, tag).await?;
            let ids: Vec<String> = batch.items.iter().map(|t| t.rri.id.clone()).collect();
            let load = PlaybackAction::LoadPagedSongs(source, batch).into();
            Ok(playback_actions(load, &ids, shuffle))
        });
    }

    /// Play a pinned artist's top tracks, or shuffle them.
    pub(super) fn play_artist(&self, id: String, shuffle: bool) {
        let api = self.app_model.api();
        let source = SongsSource::Artist(id.clone());
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let tracks = api.get_artist_top_tracks(&id, tag).await?;
            let ids: Vec<String> = tracks.iter().map(|t| t.rri.id.clone()).collect();
            let load = PlaybackAction::LoadContextSongs(source, tracks).into();
            Ok(playback_actions(load, &ids, shuffle))
        });
    }

    /// Whether a pinned album, artist or track is known to be in the user's
    /// library. Only the loaded pages of the library are checked.
    pub(super) fn is_in_library(&self, kind: settings::PinnedKind, id: &str) -> bool {
        let state = self.app_model.get_state();
        let Some(home) = state.browser.home_state() else {
            return false;
        };
        match kind {
            settings::PinnedKind::Playlist => home.playlists.iter().any(|c| c.id() == id),
            settings::PinnedKind::Album => home.albums.iter().any(|c| c.id() == id),
            settings::PinnedKind::Artist => home.artists.iter().any(|c| c.id() == id),
            settings::PinnedKind::Track => home.saved_tracks.get(id).is_some(),
        }
    }

    /// Remove a pinned album, artist or track from the user's library, and
    /// unpin it as it can no longer be pinned from its detail page.
    pub(super) fn remove_from_library(&self, kind: settings::PinnedKind, id: String) {
        if kind == settings::PinnedKind::Playlist {
            self.unfollow_playlist(id);
            return;
        }
        let user_id = self.logged_user_id();
        let api = self.app_model.api();
        dispatch_api_call_many(&self.dispatcher, move || async move {
            let removed: AppAction = match kind {
                settings::PinnedKind::Album => {
                    api.remove_albums(&id).await?;
                    BrowserAction::UnsaveAlbum(id.clone()).into()
                }
                settings::PinnedKind::Artist => {
                    api.unfollow_artists(&id).await?;
                    BrowserAction::UnfollowArtist(id.clone()).into()
                }
                _ => {
                    api.remove_tracks(vec![id.clone()]).await?;
                    BrowserAction::RemoveSavedTracks(vec![id.clone()]).into()
                }
            };
            let mut actions = vec![removed];
            if user_id.is_some_and(|user_id| settings::unpin_object(&user_id, kind, &id)) {
                actions.push(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
            }
            Ok(actions)
        });
    }

    pub(super) fn navigate(&self, dest: SidebarDestination) {
        let actions = match dest {
            SidebarDestination::Library
            | SidebarDestination::SavedTracks
            | SidebarDestination::NowPlaying
            | SidebarDestination::SavedPlaylists
            | SidebarDestination::SavedArtists => {
                vec![
                    BrowserAction::NavigationPopTo(ScreenName::Home).into(),
                    BrowserAction::SetHomeVisiblePage(dest.id()).into(),
                ]
            }
            SidebarDestination::Playlist(PlaylistSummary { id, .. }) => {
                vec![AppAction::ViewPlaylist(id)]
            }
            SidebarDestination::Album { id, .. } => vec![AppAction::ViewAlbum(id)],
            SidebarDestination::Artist { id, .. } => vec![AppAction::ViewArtist(id)],
            SidebarDestination::Track { id, .. } => {
                self.view_track_album(id);
                return;
            }
        };
        self.dispatcher.dispatch_many(actions);
    }

    /// Open the album page of a track, as there is no track detail page.
    fn view_track_album(&self, id: String) {
        let api = self.app_model.api();
        dispatch_api_read(&self.dispatcher, move |tag| async move {
            let track = api.get_track(&id, tag).await?;
            Ok(AppAction::ViewAlbum(
                track.album.map(|a| a.rri.id).unwrap_or_default(),
            ))
        });
    }

    /// Play a single track in the context of its album.
    ///
    /// `PlaybackAction::Load` only plays songs already in the queue, so the
    /// album is queued first.
    pub(super) fn play_track(&self, id: String) {
        let api = self.app_model.api();
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let track = api.get_track(&id, tag).await?;
            let album_id = track.album.as_ref().map(|a| a.rri.id.clone());
            let mut actions: Vec<AppAction> = vec![PlaybackAction::SetShuffled(false).into()];
            match album_id {
                Some(album_id) => {
                    let batch = api.get_album_tracks(&album_id, 0, 50, tag).await?;
                    let source = SongsSource::Album(album_id);
                    if batch.items.iter().any(|t| t.rri.id == id) {
                        actions.push(PlaybackAction::LoadPagedSongs(source, batch).into());
                    } else {
                        actions.push(PlaybackAction::LoadContextSongs(source, vec![track]).into());
                    }
                }
                None => {
                    #[allow(deprecated)]
                    actions.push(PlaybackAction::LoadSongs(vec![track]).into());
                }
            }
            actions.push(PlaybackAction::Load(id).into());
            Ok(actions)
        });
    }

    pub(super) fn toggle_pin_playlist(&self, id: &str) {
        let Some(user_id) = self.logged_user_id() else {
            return;
        };
        let changed = if settings::is_object_pinned(&user_id, id, settings::PinnedKind::Playlist) {
            settings::unpin_object(&user_id, settings::PinnedKind::Playlist, id)
        } else if !self.is_in_library(settings::PinnedKind::Playlist, id) {
            // Only saved playlists can be pinned.
            return;
        } else {
            let title = self.saved_playlist_title(id);
            settings::pin_object(&user_id, settings::PinnedKind::Playlist, id, title)
        };
        if changed {
            self.dispatcher
                .dispatch(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
        }
    }

    pub(super) fn unpin(&self, kind: settings::PinnedKind, id: &str) {
        let Some(user_id) = self.logged_user_id() else {
            return;
        };
        if settings::unpin_object(&user_id, kind, id) {
            self.dispatcher
                .dispatch(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
        }
    }

    fn saved_playlist_title(&self, id: &str) -> Option<String> {
        self.app_model
            .get_state()
            .browser
            .home_state()?
            .playlists
            .iter()
            .find(|c| c.id() == id)
            .map(|c| c.title())
    }

    fn prune_stale_pins(&self) {
        if !is_enabled(FeatureFlag::PinnedObjects) {
            return;
        }
        let Some(user_id) = self.logged_user_id() else {
            return;
        };
        // Only playlist pins can be pruned reliably: the saved-playlists list
        // is the authoritative set of playlists that still exist. Albums,
        // artists and tracks are pinned from detail pages that may be opened
        // before the corresponding home lists are loaded, so pruning against
        // those lists could drop valid pins.
        //
        // Even for playlists, prune only once every page has been loaded,
        // otherwise pins past the loaded pages (or all pins, before the first
        // page arrives) would be dropped.
        let fully_loaded = self
            .app_model
            .get_state()
            .browser
            .home_state()
            .is_some_and(|home| home.next_playlists_page.next_offset.is_none());
        if !fully_loaded {
            return;
        }
        let saved_ids: Vec<String> = self
            .get_playlists()
            .into_iter()
            .filter_map(|destination| {
                if let SidebarDestination::Playlist(summary) = destination {
                    Some(summary.id)
                } else {
                    None
                }
            })
            .collect();
        let _ =
            settings::prune_pinned_objects(&user_id, settings::PinnedKind::Playlist, &saved_ids);
    }

    /// Rebuild the pinned and playlist rows of `list_store`.
    ///
    /// The store holds, in order: the fixed library entries, the pinned rows,
    /// the fixed "Playlists" header (and optional "New Playlist" entry), then
    /// the playlists. `num_fixed_entries` counts the fixed rows only. The
    /// fixed rows are kept rather than rebuilt, as the "New Playlist" row
    /// hosts the create-playlist popover.
    pub fn apply_sidebar_items(&self, list_store: &gio::ListStore, num_fixed_entries: u32) {
        self.prune_stale_pins();
        let (pinned, playlists) = self.build_sidebar_items();
        let position = |id: &str| {
            (0..list_store.n_items()).find(|&i| {
                list_store
                    .item(i)
                    .and_downcast::<SidebarItem>()
                    .is_some_and(|item| item.id() == id)
            })
        };
        let Some(header) = position(SAVED_PLAYLISTS_SECTION) else {
            return;
        };
        let pinned_start = position(PINNED_SECTION).unwrap_or(header);
        let num_header_entries = num_fixed_entries.saturating_sub(pinned_start);
        list_store.splice(pinned_start, header - pinned_start, pinned.as_slice());

        let playlists_start = pinned_start + pinned.len() as u32 + num_header_entries;
        list_store.splice(
            playlists_start,
            list_store.n_items().saturating_sub(playlists_start),
            playlists.as_slice(),
        );
    }

    fn pinned_title_for(&self, object: &settings::PinnedObject) -> String {
        let fallback = match object.kind {
            settings::PinnedKind::Playlist => gettext("Pinned Playlist"),
            settings::PinnedKind::Album => gettext("Pinned Album"),
            settings::PinnedKind::Artist => gettext("Pinned Artist"),
            settings::PinnedKind::Track => gettext("Pinned Track"),
        };
        let fallback = object.title.clone().unwrap_or(fallback);
        let state = self.app_model.get_state();
        let Some(home) = state.browser.home_state() else {
            return fallback;
        };
        match object.kind {
            settings::PinnedKind::Playlist => home
                .playlists
                .iter()
                .find(|c| c.id() == object.id)
                .map(|c| c.title())
                .unwrap_or(fallback),
            settings::PinnedKind::Album => home
                .albums
                .iter()
                .find(|c| c.id() == object.id)
                .map(|c| c.title())
                .unwrap_or(fallback),
            settings::PinnedKind::Artist => home
                .artists
                .iter()
                .find(|c| c.id() == object.id)
                .map(|c| c.title())
                .unwrap_or(fallback),
            settings::PinnedKind::Track => home
                .saved_tracks
                .get(&object.id)
                .map(|sm| sm.description().title.clone())
                .unwrap_or(fallback),
        }
    }

    fn to_pinned_item(&self, object: settings::PinnedObject) -> SidebarItem {
        let title = self.pinned_title_for(&object);
        SidebarItem::from_destination(match object.kind {
            settings::PinnedKind::Playlist => SidebarDestination::Playlist(PlaylistSummary {
                id: object.id,
                title,
            }),
            settings::PinnedKind::Album => SidebarDestination::Album {
                id: object.id,
                title,
            },
            settings::PinnedKind::Artist => SidebarDestination::Artist {
                id: object.id,
                title,
            },
            settings::PinnedKind::Track => SidebarDestination::Track {
                id: object.id,
                title,
            },
        })
    }

    /// Build the pinned rows (with their section header) and the playlist rows.
    fn build_sidebar_items(&self) -> (Vec<SidebarItem>, Vec<SidebarItem>) {
        let mut pinned_items = Vec::new();
        let mut items = Vec::new();
        let pinned_enabled = is_enabled(FeatureFlag::PinnedObjects);
        let playlists = self.get_playlists();
        let pinned: Vec<settings::PinnedObject> = self
            .logged_user_id()
            .map(|user_id| settings::get_pinned_objects(&user_id))
            .unwrap_or_default();
        let pinned_playlist_ids: Vec<String> = pinned
            .iter()
            .filter(|o| o.kind == settings::PinnedKind::Playlist)
            .map(|o| o.id.clone())
            .collect();

        if pinned_enabled {
            if !pinned.is_empty() {
                pinned_items.push(SidebarItem::pinned_section());
                for object in pinned {
                    pinned_items.push(self.to_pinned_item(object));
                }
            }

            let mut unpinned = Vec::new();
            for p in playlists {
                if let SidebarDestination::Playlist(ref summary) = p {
                    if pinned_playlist_ids.contains(&summary.id) {
                        continue;
                    }
                }
                unpinned.push(p);
            }

            items.extend(unpinned.into_iter().map(SidebarItem::from_destination));
        } else {
            items.extend(playlists.into_iter().map(SidebarItem::from_destination));
        }

        (pinned_items, items)
    }
}
