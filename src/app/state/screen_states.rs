use std::borrow::Cow;
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};

use super::{
    pagination::Pagination, BrowserAction, BrowserEvent, PaginationTarget, UpdatableState,
};
use crate::app::components::card::IMAGE_SIZE;
use crate::app::models::*;
use crate::app::ListStore;

/// Number of cards (albums, playlists) to load per batch.
pub const CARD_BATCH_SIZE: usize = 50;

#[derive(Clone, Debug)]
pub enum ScreenName {
    Home,
    AlbumDetails(String),
    Search,
    Artist(String),
    PlaylistDetails(String),
    User(String),
}

impl ScreenName {
    pub fn identifier(&self) -> Cow<str> {
        match self {
            Self::Home => Cow::Borrowed("home"),
            Self::AlbumDetails(s) => Cow::Owned(format!("album_{s}")),
            Self::Search => Cow::Borrowed("search"),
            Self::Artist(s) => Cow::Owned(format!("artist_{s}")),
            Self::PlaylistDetails(s) => Cow::Owned(format!("playlist_{s}")),
            Self::User(s) => Cow::Owned(format!("user_{s}")),
        }
    }
}

impl PartialEq for ScreenName {
    fn eq(&self, other: &Self) -> bool {
        self.identifier() == other.identifier()
    }
}

impl Eq for ScreenName {}

// ALBUM details
pub struct DetailsState {
    pub id: String,
    pub name: ScreenName,
    pub album: Option<Album>,
    pub is_liked: bool,
    // Read the songs from here, not `album` (which carries no track list).
    pub songs: SongListModel,
    pub next_tracks_page: Pagination<String>,
}

impl DetailsState {
    pub fn new(id: String) -> Self {
        Self {
            id: id.clone(),
            name: ScreenName::AlbumDetails(id.clone()),
            album: None,
            is_liked: false,
            songs: SongListModel::new(50),
            next_tracks_page: Pagination::new(id, 50),
        }
    }
}

impl UpdatableState for DetailsState {
    type Action = BrowserAction;
    type Event = BrowserEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        match action.as_ref() {
            BrowserAction::SetAlbumInfo(album) if album.rri.id == self.id => {
                let id = album.rri.id.clone();
                self.album = Some((**album).clone());
                vec![BrowserEvent::AlbumDetailsLoaded(id)]
            }
            BrowserAction::SetAlbumTracks(id, batch) if id == &self.id => {
                self.songs.add(*batch.clone()).commit();
                self.next_tracks_page.reset_count(self.songs.partial_len());
                vec![BrowserEvent::AlbumTracksAppended(id.clone())]
            }
            BrowserAction::SetAlbumLikedStatus(id, is_liked) if id == &self.id => {
                self.is_liked = *is_liked;
                if *is_liked {
                    vec![BrowserEvent::AlbumSaved(id.clone())]
                } else {
                    vec![BrowserEvent::AlbumUnsaved(id.clone())]
                }
            }
            BrowserAction::AppendAlbumTracks(id, batch) if id == &self.id => {
                self.next_tracks_page.set_loaded_count(batch.items.len());
                self.songs.add(*batch.clone()).commit();
                vec![BrowserEvent::AlbumTracksAppended(id.clone())]
            }
            BrowserAction::PlaySong(id) => {
                vec![BrowserEvent::SongPlaybackRequested(id.clone())]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::AlbumTracks(id)) if id == &self.id => {
                self.next_tracks_page.next_offset_take();
                vec![]
            }
            BrowserAction::PageRequestFailed(PaginationTarget::AlbumTracks(id), offset)
                if id == &self.id =>
            {
                self.next_tracks_page.restore_offset(*offset);
                vec![]
            }
            BrowserAction::SaveAlbum(album) if album.rri.id == self.id => {
                self.is_liked = true;
                vec![BrowserEvent::AlbumSaved(album.rri.id.clone())]
            }
            BrowserAction::UnsaveAlbum(id) if id == &self.id => {
                self.is_liked = false;
                vec![BrowserEvent::AlbumUnsaved(id.clone())]
            }
            _ => vec![],
        }
    }
}

fn replace_card(cards: &mut ListStore<CardModel>, id: &str, change: impl FnOnce(&CardModel)) {
    let Some(position) = cards.iter().position(|card| card.id() == id) else {
        return;
    };
    let replacement = cards.get(position as u32).duplicate();
    change(&replacement);
    cards.remove(position as u32);
    cards.insert(position as u32, replacement);
}

fn not_listed<'a, T>(
    cards: &ListStore<CardModel>,
    items: &'a [T],
    id: impl Fn(&T) -> &String,
) -> Vec<&'a T> {
    let listed: HashSet<String> = cards.iter().map(|card| card.id()).collect();
    items
        .iter()
        .filter(|item| !listed.contains(id(item)))
        .collect()
}

pub struct PlaylistDetailsState {
    pub id: String,
    pub name: ScreenName,
    pub playlist: Option<Playlist>,
    // Read the songs from here, not content (won't get more than the initial batch of songs)
    pub songs: SongListModel,
    pub next_tracks_page: Pagination<String>,
}

impl PlaylistDetailsState {
    pub fn new(id: String) -> Self {
        Self {
            id: id.clone(),
            name: ScreenName::PlaylistDetails(id.clone()),
            playlist: None,
            songs: SongListModel::new(50),
            next_tracks_page: Pagination::new(id, 50),
        }
    }
}

impl UpdatableState for PlaylistDetailsState {
    type Action = BrowserAction;
    type Event = BrowserEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        match action.as_ref() {
            BrowserAction::SetPlaylistDetails(playlist, song_batch)
                if playlist.rri.id == self.id =>
            {
                let id = playlist.rri.id.clone();
                self.songs.add(*song_batch.clone()).commit();
                self.next_tracks_page.reset_count(self.songs.partial_len());
                self.playlist = Some(*playlist.clone());
                vec![BrowserEvent::PlaylistDetailsLoaded(id)]
            }
            BrowserAction::UpdatePlaylistName(PlaylistSummary { id, title }) if id == &self.id => {
                if let Some(p) = self.playlist.as_mut() {
                    p.title = title.clone();
                }
                vec![BrowserEvent::PlaylistDetailsLoaded(self.id.clone())]
            }
            BrowserAction::UpdatePlaylistInfo(id, description, public) if id == &self.id => {
                let Some(p) = self.playlist.as_mut() else {
                    return vec![];
                };
                p.description = description.clone();
                let relabelled = public.is_some() && p.public != *public;
                p.public = public.or(p.public);
                if relabelled {
                    vec![BrowserEvent::PlaylistDetailsLoaded(self.id.clone())]
                } else {
                    vec![]
                }
            }
            BrowserAction::UpdatePlaylistArt(id, art) if id == &self.id => {
                match self.playlist.as_mut() {
                    Some(p) if &p.art != art => {
                        p.art = art.clone();
                        vec![BrowserEvent::PlaylistDetailsLoaded(self.id.clone())]
                    }
                    _ => vec![],
                }
            }
            BrowserAction::AppendPlaylistTracks(id, song_batch) if id == &self.id => {
                self.next_tracks_page
                    .set_loaded_count(song_batch.items.len());
                self.songs.add(*song_batch.clone()).commit();
                vec![BrowserEvent::PlaylistTracksAppended(id.clone())]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::PlaylistTracks(id))
                if id == &self.id =>
            {
                self.next_tracks_page.next_offset_take();
                vec![]
            }
            BrowserAction::PageRequestFailed(PaginationTarget::PlaylistTracks(id), offset)
                if id == &self.id =>
            {
                self.next_tracks_page.restore_offset(*offset);
                vec![]
            }
            BrowserAction::AddTracksToPlaylist(id, tracks) if id == &self.id => {
                if let Some(total) = self.playlist.as_mut().and_then(|p| p.total_tracks.as_mut()) {
                    *total += tracks.len() as u32;
                }
                if self.next_tracks_page.next_offset.is_some() {
                    return vec![BrowserEvent::PlaylistDetailsLoaded(self.id.clone())];
                }
                let batch_size = self.next_tracks_page.batch_size;
                let mut listed = self.songs.collect();
                let first_changed = listed.len() / batch_size * batch_size;
                let total = listed.len() + tracks.len();
                let mut changed = listed.split_off(first_changed);
                changed.extend(tracks.iter().cloned());
                for (i, page) in changed.chunks(batch_size).enumerate() {
                    self.songs
                        .add(Page {
                            items: page.to_vec(),
                            offset: Some(first_changed + i * batch_size),
                            total: Some(total),
                            next_cursor: None,
                        })
                        .commit();
                }
                vec![
                    BrowserEvent::PlaylistTracksAppended(self.id.clone()),
                    BrowserEvent::PlaylistDetailsLoaded(self.id.clone()),
                ]
            }
            BrowserAction::RemoveTracksFromPlaylist(id, uris) if id == &self.id => {
                let uris: HashSet<&String> = uris.iter().collect();
                let ids: Vec<String> = self
                    .songs
                    .collect()
                    .into_iter()
                    .filter(|track| track.rri.uri.as_ref().is_some_and(|uri| uris.contains(uri)))
                    .map(|track| track.rri.id)
                    .collect();
                self.songs.remove(&ids[..]).commit();
                if let Some(total) = self.playlist.as_mut().and_then(|p| p.total_tracks.as_mut()) {
                    *total = total.saturating_sub(ids.len() as u32);
                }
                vec![BrowserEvent::PlaylistTracksRemoved(self.id.clone())]
            }
            BrowserAction::SavePlaylist(playlist) if playlist.rri.id == self.id => {
                vec![BrowserEvent::PlaylistSaved(self.id.clone())]
            }
            BrowserAction::UnsavePlaylist(id) if id == &self.id => {
                vec![BrowserEvent::PlaylistUnsaved(self.id.clone())]
            }
            _ => vec![],
        }
    }
}

pub struct ArtistState {
    pub id: String,
    pub name: ScreenName,
    pub artist: Option<String>,
    pub photo: Option<ImageSet>,
    pub is_followed: bool,
    pub next_page: Pagination<String>,
    pub albums: ListStore<CardModel>,
    pub top_tracks: SongListModel,
}

impl ArtistState {
    pub fn new(id: String) -> Self {
        Self {
            id: id.clone(),
            name: ScreenName::Artist(id.clone()),
            artist: None,
            photo: None,
            is_followed: false,
            next_page: Pagination::new(id, CARD_BATCH_SIZE),
            albums: ListStore::new(),
            top_tracks: SongListModel::new(10),
        }
    }
}

impl UpdatableState for ArtistState {
    type Action = BrowserAction;
    type Event = BrowserEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        match action.as_ref() {
            BrowserAction::SetArtistInfo(artist) if artist.rri.id == self.id => {
                self.artist = Some(artist.name.clone());
                self.photo = Some(artist.art.clone());
                vec![BrowserEvent::ArtistDetailsUpdated(self.id.clone())]
            }
            BrowserAction::SetArtistReleases(id, albums) if id == &self.id => {
                self.albums.replace_all(albums.iter().map(|a| a.into()));
                self.next_page.reset_count(self.albums.len());
                vec![BrowserEvent::ArtistDetailsUpdated(id.clone())]
            }
            BrowserAction::SetArtistTopTracks(id, top_tracks) if id == &self.id => {
                let mut top_tracks = top_tracks.clone();
                top_tracks.truncate(10);
                self.top_tracks.append(top_tracks).commit();
                vec![BrowserEvent::ArtistDetailsUpdated(id.clone())]
            }
            BrowserAction::SetArtistFollowedStatus(id, is_followed) if id == &self.id => {
                self.is_followed = *is_followed;
                vec![BrowserEvent::ArtistDetailsUpdated(id.clone())]
            }
            BrowserAction::AppendArtistReleases(id, albums) if id == &self.id => {
                self.next_page.set_loaded_count(albums.len());
                self.albums.extend(albums.iter().map(|a| a.into()));
                vec![BrowserEvent::ArtistDetailsUpdated(self.id.clone())]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::ArtistReleases(id))
                if id == &self.id =>
            {
                self.next_page.next_offset_take();
                vec![]
            }
            BrowserAction::FollowArtist(artist) if artist.rri.id == self.id => {
                self.is_followed = true;
                vec![BrowserEvent::ArtistDetailsUpdated(self.id.clone())]
            }
            BrowserAction::UnfollowArtist(id) if id == &self.id => {
                self.is_followed = false;
                vec![BrowserEvent::ArtistDetailsUpdated(self.id.clone())]
            }
            _ => vec![],
        }
    }
}

// The "home" represents screens visible initially (saved albums, saved playlists, saved tracks)
pub struct HomeState {
    pub name: ScreenName,
    pub visible_page: &'static str,
    pub next_albums_page: Pagination<()>,
    pub albums: ListStore<CardModel>,
    pub next_playlists_page: Pagination<()>,
    pub playlists: ListStore<CardModel>,
    pub next_saved_tracks_page: Pagination<()>,
    pub saved_tracks: SongListModel,
    /// Total number of saved tracks reported by the API, known from the first
    /// page onwards. Display only - pagination still stops on a short page.
    pub saved_tracks_total: Option<usize>,
    pub liked_status: HashMap<String, bool>,
    pub artists: ListStore<CardModel>,
    pub artists_cursor: Option<String>,
}

impl Default for HomeState {
    fn default() -> Self {
        Self {
            name: ScreenName::Home,
            visible_page: "library",
            next_albums_page: Pagination::new((), CARD_BATCH_SIZE),
            albums: ListStore::new(),
            next_playlists_page: Pagination::new((), CARD_BATCH_SIZE),
            playlists: ListStore::new(),
            next_saved_tracks_page: Pagination::new((), 50),
            saved_tracks: SongListModel::new(50),
            saved_tracks_total: None,
            liked_status: HashMap::new(),
            artists: ListStore::new(),
            artists_cursor: Some(String::new()),
        }
    }
}

impl HomeState {
    pub fn is_track_liked(&self, id: &str) -> bool {
        self.liked_status
            .get(id)
            .copied()
            .unwrap_or_else(|| self.saved_tracks.get(id).is_some())
    }

    fn record_liked_statuses(&mut self, statuses: impl Iterator<Item = (String, bool)>) -> bool {
        let mut changed = false;
        for (id, liked) in statuses {
            if self.liked_status.contains_key(&id) {
                continue;
            }
            changed |= self.is_track_liked(&id) != liked;
            self.liked_status.insert(id, liked);
        }
        changed
    }

    fn mark_liked(&mut self, tracks: &[Track]) -> bool {
        let mut changed = false;
        for track in tracks {
            changed |= self.liked_status.insert(track.rri.id.clone(), true) == Some(false);
        }
        changed
    }

    fn set_playlist_card_art(&mut self, id: &str, art: &ImageSet) -> Vec<BrowserEvent> {
        let image = art.best_for_width(IMAGE_SIZE).map(str::to_owned);
        let outdated = self
            .playlists
            .iter()
            .any(|card| card.id() == id && card.image() != image);
        if !outdated {
            return vec![];
        }
        replace_card(&mut self.playlists, id, |card| {
            card.set_image_url(image.as_deref())
        });
        vec![BrowserEvent::SavedPlaylistsUpdated]
    }
}

impl UpdatableState for HomeState {
    type Action = BrowserAction;
    type Event = BrowserEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        let status_changed =
            self.record_liked_statuses(liked_statuses(delivered_tracks(action.as_ref())));
        let mut events = match action.as_ref() {
            BrowserAction::SetHomeVisiblePage(page) => {
                self.visible_page = *page;
                vec![BrowserEvent::HomeVisiblePageChanged(page)]
            }
            BrowserAction::SetLibraryContent(content) => {
                if !self.albums.eq(content, |a, b| a.id() == b.rri.id) {
                    self.albums.replace_all(content.iter().map(|a| a.into()));
                    self.next_albums_page.reset_count(self.albums.len());
                    vec![BrowserEvent::LibraryUpdated]
                } else {
                    vec![]
                }
            }
            BrowserAction::PrependPlaylistsContent(content) => {
                self.playlists.prepend(content.iter().map(|a| a.into()));
                vec![BrowserEvent::SavedPlaylistsUpdated]
            }
            BrowserAction::AppendLibraryContent(content) => {
                self.next_albums_page.set_loaded_count(content.len());
                let new = not_listed(&self.albums, content, |a| &a.rri.id);
                self.albums.extend(new.into_iter().map(|a| a.into()));
                vec![BrowserEvent::LibraryUpdated]
            }
            BrowserAction::SaveAlbum(album) => {
                let album_id = album.rri.id.clone();
                let already_present = self.albums.iter().any(|a| a.id() == album_id);
                if already_present {
                    vec![]
                } else {
                    self.albums.insert(0, (*album.clone()).into());
                    self.next_albums_page.increment();
                    vec![BrowserEvent::LibraryUpdated]
                }
            }
            BrowserAction::UnsaveAlbum(id) => {
                let position = self.albums.iter().position(|a| a.id() == *id);
                if let Some(position) = position {
                    self.albums.remove(position as u32);
                    self.next_albums_page.decrement();
                    vec![BrowserEvent::LibraryUpdated]
                } else {
                    vec![]
                }
            }
            BrowserAction::SetPlaylistsContent(content) => {
                if !self.playlists.eq(content, |a, b| a.id() == b.rri.id) {
                    self.playlists.replace_all(content.iter().map(|a| a.into()));
                    self.next_playlists_page.reset_count(self.playlists.len());
                    vec![BrowserEvent::SavedPlaylistsUpdated]
                } else {
                    vec![]
                }
            }
            BrowserAction::AppendPlaylistsContent(content) => {
                self.next_playlists_page.set_loaded_count(content.len());
                let new = not_listed(&self.playlists, content, |p| &p.rri.id);
                self.playlists.extend(new.into_iter().map(|p| p.into()));
                vec![BrowserEvent::SavedPlaylistsUpdated]
            }
            BrowserAction::UpdatePlaylistName(PlaylistSummary { id, title }) => {
                replace_card(&mut self.playlists, id, |card| {
                    card.set_title(title.clone())
                });
                vec![BrowserEvent::SavedPlaylistsUpdated]
            }
            BrowserAction::UpdatePlaylistArt(id, art) => self.set_playlist_card_art(id, art),
            BrowserAction::SetPlaylistDetails(playlist, _) => {
                self.set_playlist_card_art(&playlist.rri.id, &playlist.art)
            }
            BrowserAction::SavePlaylist(playlist) => {
                let already_present = self.playlists.iter().any(|p| p.id() == playlist.rri.id);
                if already_present {
                    vec![]
                } else {
                    self.playlists.insert(0, (&**playlist).into());
                    self.next_playlists_page.increment();
                    vec![BrowserEvent::SavedPlaylistsUpdated]
                }
            }
            BrowserAction::RemovePlaylist(id) | BrowserAction::UnsavePlaylist(id) => {
                let position = self.playlists.iter().position(|p| p.id() == *id);
                if let Some(position) = position {
                    self.playlists.remove(position as u32);
                    self.next_playlists_page.decrement();
                    vec![BrowserEvent::SavedPlaylistsUpdated]
                } else {
                    vec![]
                }
            }
            BrowserAction::AppendSavedTracks(song_batch) => {
                self.next_saved_tracks_page
                    .set_loaded_count(song_batch.items.len());
                if let Some(total) = song_batch.total {
                    self.saved_tracks_total = Some(total);
                }
                let status_changed = self.mark_liked(&song_batch.items);
                if self.saved_tracks.add(*song_batch.clone()).commit() || status_changed {
                    vec![BrowserEvent::SavedTracksUpdated]
                } else {
                    vec![]
                }
            }
            BrowserAction::SetSavedTracks(song_batch) => {
                let song_batch = *song_batch.clone();
                let len = song_batch.items.len();
                let total = song_batch.total;
                let status_changed = self.mark_liked(&song_batch.items);
                let list_changed = self
                    .saved_tracks
                    .clear()
                    .and(|s| s.add(song_batch))
                    .commit();
                if list_changed {
                    self.next_saved_tracks_page.reset_count(len);
                    self.saved_tracks_total = total;
                }
                if list_changed || status_changed {
                    vec![BrowserEvent::SavedTracksUpdated]
                } else {
                    vec![]
                }
            }
            BrowserAction::SetTracksLikedStatus(statuses) => {
                if self.record_liked_statuses(statuses.iter().cloned()) {
                    vec![BrowserEvent::SavedTracksUpdated]
                } else {
                    vec![]
                }
            }
            BrowserAction::ClearTracksLikedStatus => {
                self.liked_status.clear();
                vec![BrowserEvent::SavedTracksUpdated]
            }
            BrowserAction::SaveTracks(tracks) => {
                for track in tracks {
                    self.liked_status.insert(track.rri.id.clone(), true);
                }
                self.saved_tracks.prepend(tracks.clone()).commit();
                if let Some(total) = self.saved_tracks_total.as_mut() {
                    *total = total.saturating_add(tracks.len());
                }
                vec![BrowserEvent::SavedTracksUpdated]
            }
            BrowserAction::RemoveSavedTracks(tracks) => {
                for id in tracks {
                    self.liked_status.insert(id.clone(), false);
                }
                self.saved_tracks.remove(&tracks[..]).commit();
                if let Some(total) = self.saved_tracks_total.as_mut() {
                    *total = total.saturating_sub(tracks.len());
                }
                vec![BrowserEvent::SavedTracksUpdated]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::SavedAlbums) => {
                self.next_albums_page.next_offset_take();
                vec![]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::SavedPlaylists) => {
                self.next_playlists_page.next_offset_take();
                vec![]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::SavedTracks) => {
                self.next_saved_tracks_page.next_offset_take();
                vec![]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::SavedArtists) => {
                self.artists_cursor = None;
                vec![]
            }
            BrowserAction::SetSavedArtists(artists, cursor) => {
                self.artists.replace_all(artists.iter().map(|a| a.into()));
                self.artists_cursor = cursor.clone();
                vec![BrowserEvent::SavedArtistsUpdated]
            }
            BrowserAction::AppendSavedArtists(artists, cursor) => {
                self.artists.extend(artists.iter().map(|a| a.into()));
                self.artists_cursor = cursor.clone();
                vec![BrowserEvent::SavedArtistsUpdated]
            }
            BrowserAction::FollowArtist(artist) => {
                let already_present = self.artists.iter().any(|a| a.id() == artist.rri.id);
                if already_present {
                    vec![]
                } else {
                    self.artists.insert(0, artist.into());
                    vec![BrowserEvent::SavedArtistsUpdated]
                }
            }
            BrowserAction::UnfollowArtist(id) => {
                let position = self.artists.iter().position(|a| a.id() == *id);
                if let Some(position) = position {
                    self.artists.remove(position as u32);
                    vec![BrowserEvent::SavedArtistsUpdated]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        };
        if status_changed && !events.contains(&BrowserEvent::SavedTracksUpdated) {
            events.push(BrowserEvent::SavedTracksUpdated);
        }
        events
    }
}

fn delivered_tracks(action: &BrowserAction) -> &[Track] {
    match action {
        BrowserAction::SetAlbumTracks(_, page)
        | BrowserAction::AppendAlbumTracks(_, page)
        | BrowserAction::SetPlaylistDetails(_, page)
        | BrowserAction::AppendPlaylistTracks(_, page) => &page.items,
        BrowserAction::SetSearchResults(results)
        | BrowserAction::SetSearchScopeResults(_, _, results)
        | BrowserAction::AppendSearchScopeResults(_, _, results) => &results.tracks.items,
        BrowserAction::SetArtistTopTracks(_, tracks) => tracks,
        _ => &[],
    }
}

pub fn liked_statuses(tracks: &[Track]) -> impl Iterator<Item = (String, bool)> + '_ {
    tracks
        .iter()
        .filter_map(|track| Some((track.rri.id.clone(), track.saved?)))
}

pub struct SearchState {
    pub name: ScreenName,
    pub query: String,
    /// Active filter. `None` shows the combined multi-section results.
    pub filter: Option<SearchType>,
    /// Results for the combined ("all") view.
    pub results: SearchResults,
    /// Card results for a scoped card view (artists/albums/playlists).
    pub scope_cards: ListStore<CardModel>,
    /// Track results for a scoped track view.
    pub scope_tracks: SongListModel,
    /// Pagination for the scoped view.
    pub scope_page: Pagination<String>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            name: ScreenName::Search,
            query: String::new(),
            filter: None,
            results: SearchResults::default(),
            scope_cards: ListStore::new(),
            scope_tracks: SongListModel::new(CARD_BATCH_SIZE as u32),
            scope_page: Pagination::new(String::new(), CARD_BATCH_SIZE),
        }
    }
}

impl SearchState {
    fn reset_scope(&mut self) {
        self.scope_cards.replace_all(std::iter::empty());
        self.scope_tracks.clear().commit();
        self.scope_page = Pagination::new(self.query.clone(), CARD_BATCH_SIZE);
    }
}

impl UpdatableState for SearchState {
    type Action = BrowserAction;
    type Event = BrowserEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        match action.as_ref() {
            BrowserAction::Search(query) if query != &self.query => {
                self.query = query.clone();
                self.reset_scope();
                vec![BrowserEvent::SearchUpdated]
            }
            BrowserAction::SetSearchFilter(filter) if filter != &self.filter => {
                self.filter = *filter;
                self.reset_scope();
                vec![BrowserEvent::SearchFilterChanged]
            }
            BrowserAction::SetSearchResults(results) => {
                self.results = *results.clone();
                vec![BrowserEvent::SearchResultsUpdated]
            }
            BrowserAction::SetSearchScopeResults(search_type, query, results)
                if Some(*search_type) == self.filter && query == &self.query =>
            {
                match search_type {
                    SearchType::Tracks => {
                        self.scope_tracks.clear().commit();
                        self.scope_tracks.add(results.tracks.clone()).commit();
                        self.scope_page.reset_count(self.scope_tracks.partial_len());
                    }
                    other => {
                        let cards = cards_from_results(*other, results);
                        self.scope_cards.replace_all(cards.into_iter());
                        self.scope_page.reset_count(self.scope_cards.len());
                    }
                }
                vec![BrowserEvent::SearchScopeUpdated(*search_type)]
            }
            BrowserAction::AppendSearchScopeResults(search_type, query, results)
                if Some(*search_type) == self.filter && query == &self.query =>
            {
                match search_type {
                    SearchType::Tracks => {
                        self.scope_page.set_loaded_count(results.tracks.items.len());
                        self.scope_tracks.add(results.tracks.clone()).commit();
                    }
                    other => {
                        let cards = cards_from_results(*other, results);
                        let loaded = cards.len();
                        self.scope_cards.extend(cards.into_iter());
                        self.scope_page.set_loaded_count(loaded);
                    }
                }
                vec![BrowserEvent::SearchScopeUpdated(*search_type)]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::SearchScope) => {
                self.scope_page.next_offset_take();
                vec![]
            }
            _ => vec![],
        }
    }
}

/// Extract card models for a given search type from a `SearchResults`.
fn cards_from_results(search_type: SearchType, results: &SearchResults) -> Vec<CardModel> {
    match search_type {
        SearchType::Albums => results.albums.items.iter().map(CardModel::from).collect(),
        SearchType::Artists => results.artists.items.iter().map(CardModel::from).collect(),
        SearchType::Playlists => results
            .playlists
            .items
            .iter()
            .map(CardModel::from)
            .collect(),
        // Tracks are shown as a track list, not cards.
        SearchType::Tracks => vec![],
    }
}

// Screen when we click on the name of a playlist owner
pub struct UserState {
    pub id: String,
    pub name: ScreenName,
    pub user: Option<String>,
    pub photo: Option<ImageSet>,
    pub next_page: Pagination<String>,
    pub playlists: ListStore<CardModel>,
}

impl UserState {
    pub fn new(id: String) -> Self {
        Self {
            id: id.clone(),
            name: ScreenName::User(id.clone()),
            user: None,
            photo: None,
            next_page: Pagination::new(id, CARD_BATCH_SIZE),
            playlists: ListStore::new(),
        }
    }
}

impl UpdatableState for UserState {
    type Action = BrowserAction;
    type Event = BrowserEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        match action.as_ref() {
            BrowserAction::SetUserInfo(user) if user.rri.id == self.id => {
                self.user = Some(user.display_name.clone());
                self.photo = Some(user.art.clone());
                vec![BrowserEvent::UserDetailsUpdated(user.rri.id.clone())]
            }
            BrowserAction::SetUserPlaylists(id, playlists) if id == &self.id => {
                self.playlists
                    .replace_all(playlists.iter().map(|p| p.into()));
                self.next_page.reset_count(self.playlists.len());
                vec![BrowserEvent::UserDetailsUpdated(id.clone())]
            }
            BrowserAction::AppendUserPlaylists(id, playlists) if id == &self.id => {
                self.next_page.set_loaded_count(playlists.len());
                self.playlists.extend(playlists.iter().map(|p| p.into()));
                vec![BrowserEvent::UserDetailsUpdated(self.id.clone())]
            }
            BrowserAction::ConsumeNextPage(PaginationTarget::UserPlaylists(id))
                if id == &self.id =>
            {
                self.next_page.next_offset_take();
                vec![]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_next_page_no_next() {
        let mut artist_state = ArtistState::new("id".to_owned());
        artist_state.update_with(Cow::Owned(BrowserAction::SetArtistReleases(
            "id".to_owned(),
            vec![],
        )));

        let next = artist_state.next_page;
        assert_eq!(None, next.next_offset);
    }

    #[test]
    fn test_library_page_delivered_twice_is_listed_once() {
        let page: Vec<Album> = (0..CARD_BATCH_SIZE)
            .map(|i| make_album(&format!("album{i}")))
            .collect();
        let mut home = HomeState::default();
        home.next_albums_page.next_offset_take();
        home.update_with(Cow::Owned(BrowserAction::SetLibraryContent(page.clone())));
        home.update_with(Cow::Owned(BrowserAction::AppendLibraryContent(page)));

        assert_eq!(home.albums.len(), CARD_BATCH_SIZE);
        assert_eq!(home.next_albums_page.next_offset, Some(CARD_BATCH_SIZE));
    }

    #[test]
    fn test_next_page_more() {
        let fake_album = make_album("");
        let id = "id".to_string();
        let mut artist_state = ArtistState::new(id.clone());
        artist_state.update_with(Cow::Owned(BrowserAction::SetArtistReleases(
            id.clone(),
            (0..CARD_BATCH_SIZE).map(|_| fake_album.clone()).collect(),
        )));

        let next = &artist_state.next_page;
        assert_eq!(Some(CARD_BATCH_SIZE), next.next_offset);

        artist_state.update_with(Cow::Owned(BrowserAction::AppendArtistReleases(
            id.clone(),
            vec![],
        )));

        let next = &artist_state.next_page;
        assert_eq!(None, next.next_offset);
    }

    #[test]
    fn test_album_tracks_page_request_failed_restores_offset() {
        let id = "album-id".to_string();
        let mut state = DetailsState::new(id.clone());
        state.update_with(Cow::Owned(BrowserAction::SetAlbumTracks(
            id.clone(),
            Box::new(Page {
                items: (0..50).map(|i| make_track(&format!("t{i}"))).collect(),
                offset: Some(0),
                total: None,
                next_cursor: None,
            }),
        )));
        assert_eq!(state.next_tracks_page.next_offset, Some(50));

        // ConsumeNextPage eagerly advances the offset before the request completes.
        state.update_with(Cow::Owned(BrowserAction::ConsumeNextPage(
            PaginationTarget::AlbumTracks(id.clone()),
        )));
        assert_eq!(state.next_tracks_page.next_offset, Some(100));

        // The request for offset 50 failed - it must be retried, not skipped.
        state.update_with(Cow::Owned(BrowserAction::PageRequestFailed(
            PaginationTarget::AlbumTracks(id.clone()),
            50,
        )));
        assert_eq!(state.next_tracks_page.next_offset, Some(50));
    }

    fn playlist_state_with_tracks(id: &str, count: usize) -> PlaylistDetailsState {
        let mut state = PlaylistDetailsState::new(id.to_string());
        state.update_with(Cow::Owned(BrowserAction::SetPlaylistDetails(
            Box::new(crate::app::models::Playlist {
                rri: crate::app::models::ResourceId::new(
                    riff_api::models::Provider::Spotify,
                    id.to_string(),
                ),
                total_tracks: Some(count as u32),
                ..Default::default()
            }),
            Box::new(Page {
                items: (0..count.min(50))
                    .map(|i| make_track(&format!("t{i}")))
                    .collect(),
                offset: Some(0),
                total: None,
                next_cursor: None,
            }),
        )));
        state
    }

    fn listed_ids(state: &PlaylistDetailsState) -> Vec<String> {
        state
            .songs
            .collect()
            .into_iter()
            .map(|t| t.rri.id)
            .collect()
    }

    #[test]
    fn test_adding_tracks_to_a_loaded_playlist_lists_them_last() {
        let mut state = playlist_state_with_tracks("p", 2);
        state.update_with(Cow::Owned(BrowserAction::AddTracksToPlaylist(
            "p".to_string(),
            vec![make_track("new")],
        )));
        assert_eq!(listed_ids(&state), ["t0", "t1", "new"]);
        assert_eq!(state.playlist.as_ref().unwrap().total_tracks, Some(3));

        state.update_with(Cow::Owned(BrowserAction::AddTracksToPlaylist(
            "other".to_string(),
            vec![make_track("elsewhere")],
        )));
        assert_eq!(listed_ids(&state), ["t0", "t1", "new"]);
    }

    #[test]
    fn test_adding_tracks_fills_the_last_page_and_starts_the_next() {
        let mut state = playlist_state_with_tracks("p", 49);
        state.update_with(Cow::Owned(BrowserAction::AddTracksToPlaylist(
            "p".to_string(),
            vec![make_track("a"), make_track("b")],
        )));
        let ids = listed_ids(&state);
        assert_eq!(ids.len(), 51);
        assert_eq!(ids[48..], ["t48", "a", "b"]);
    }

    #[test]
    fn test_adding_tracks_to_a_partly_loaded_playlist_only_counts_them() {
        let mut state = playlist_state_with_tracks("p", 120);
        state.update_with(Cow::Owned(BrowserAction::AddTracksToPlaylist(
            "p".to_string(),
            vec![make_track("new")],
        )));
        assert_eq!(listed_ids(&state).len(), 50);
        assert_eq!(state.playlist.as_ref().unwrap().total_tracks, Some(121));
    }

    #[test]
    fn test_loading_a_playlist_brings_its_new_cover_to_its_card() {
        let art = |url: &str| ImageSet {
            images: vec![riff_api::models::Image {
                url: url.to_string(),
                width: Some(640),
                height: Some(640),
            }],
            template: None,
        };
        let playlist = |url: &str| crate::app::models::Playlist {
            rri: crate::app::models::ResourceId::new(
                riff_api::models::Provider::Spotify,
                "p".to_string(),
            ),
            art: art(url),
            ..Default::default()
        };
        let details = |url: &str| {
            BrowserAction::SetPlaylistDetails(Box::new(playlist(url)), Box::new(Page::default()))
        };
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::PrependPlaylistsContent(vec![
            playlist("https://example.com/stand-in.png"),
        ])));

        let events = home.update_with(Cow::Owned(details("https://example.com/mosaic.jpg")));
        assert_eq!(events, vec![BrowserEvent::SavedPlaylistsUpdated]);
        assert_eq!(
            home.playlists.get(0).image().as_deref(),
            Some("https://example.com/mosaic.jpg")
        );

        let events = home.update_with(Cow::Owned(details("https://example.com/mosaic.jpg")));
        assert!(events.is_empty());
    }

    #[test]
    fn test_replacing_a_card_swaps_its_model_in_place() {
        let card = |id: &str, title: &str| {
            CardModel::new(id, None, title, "owner", None, None, Some(7), None)
        };
        let mut cards: ListStore<CardModel> = ListStore::new();
        cards.extend(vec![card("a", "A"), card("b", "B"), card("c", "C")].into_iter());
        let before = cards.get(1);

        replace_card(&mut cards, "b", |card| {
            card.set_title("Renamed".to_string())
        });

        let titles: Vec<String> = cards.iter().map(|c| c.title()).collect();
        assert_eq!(titles, ["A", "Renamed", "C"]);
        let after = cards.get(1);
        assert_ne!(before, after);
        assert_eq!(after.subtitle(), "owner");
        assert_eq!(after.insertion_position(), 7);
    }

    #[test]
    fn test_removing_tracks_from_playlist_drops_them_from_the_list() {
        let id = "playlist-id".to_string();
        let track = |n: usize| {
            let mut track = make_track(&format!("t{n}"));
            track.rri.uri = Some(format!("spotify:track:t{n}"));
            track
        };
        let mut state = PlaylistDetailsState::new(id.clone());
        state.update_with(Cow::Owned(BrowserAction::SetPlaylistDetails(
            Box::new(crate::app::models::Playlist {
                rri: crate::app::models::ResourceId::new(
                    riff_api::models::Provider::Spotify,
                    id.clone(),
                ),
                total_tracks: Some(3),
                ..Default::default()
            }),
            Box::new(Page {
                items: (0..3).map(track).collect(),
                offset: Some(0),
                total: None,
                next_cursor: None,
            }),
        )));

        let events = state.update_with(Cow::Owned(BrowserAction::RemoveTracksFromPlaylist(
            id.clone(),
            vec!["spotify:track:t1".to_string()],
        )));

        assert_eq!(events, vec![BrowserEvent::PlaylistTracksRemoved(id)]);
        let left: Vec<String> = state
            .songs
            .collect()
            .into_iter()
            .map(|t| t.rri.id)
            .collect();
        assert_eq!(left, ["t0", "t2"]);
        assert_eq!(state.playlist.unwrap().total_tracks, Some(2));
    }

    #[test]
    fn test_playlist_tracks_page_request_failed_restores_offset() {
        let id = "playlist-id".to_string();
        let mut state = PlaylistDetailsState::new(id.clone());
        state.update_with(Cow::Owned(BrowserAction::SetPlaylistDetails(
            Box::new(crate::app::models::Playlist {
                rri: crate::app::models::ResourceId::new(
                    riff_api::models::Provider::Spotify,
                    id.clone(),
                ),
                ..Default::default()
            }),
            Box::new(Page {
                items: (0..50).map(|i| make_track(&format!("t{i}"))).collect(),
                offset: Some(0),
                total: None,
                next_cursor: None,
            }),
        )));
        assert_eq!(state.next_tracks_page.next_offset, Some(50));

        state.update_with(Cow::Owned(BrowserAction::ConsumeNextPage(
            PaginationTarget::PlaylistTracks(id.clone()),
        )));
        assert_eq!(state.next_tracks_page.next_offset, Some(100));

        state.update_with(Cow::Owned(BrowserAction::PageRequestFailed(
            PaginationTarget::PlaylistTracks(id.clone()),
            50,
        )));
        assert_eq!(state.next_tracks_page.next_offset, Some(50));
    }

    fn playlist(id: &str, title: &str) -> Playlist {
        make_playlist(id, title)
    }

    fn artist_summary(id: &str, name: &str) -> Artist {
        make_artist(id, name)
    }

    #[test]
    fn test_home_save_playlist_adds_card() {
        let mut home = HomeState::default();
        let events = home.update_with(Cow::Owned(BrowserAction::SavePlaylist(Box::new(playlist(
            "pl1",
            "My Playlist",
        )))));

        assert_eq!(home.playlists.len(), 1);
        assert_eq!(home.playlists.get(0).id(), "pl1");
        assert_eq!(events, vec![BrowserEvent::SavedPlaylistsUpdated]);
    }

    #[test]
    fn test_home_save_playlist_is_idempotent() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SavePlaylist(Box::new(playlist(
            "pl1",
            "My Playlist",
        )))));
        let events = home.update_with(Cow::Owned(BrowserAction::SavePlaylist(Box::new(playlist(
            "pl1",
            "My Playlist",
        )))));

        assert_eq!(home.playlists.len(), 1);
        assert!(events.is_empty());
    }

    #[test]
    fn test_home_unsave_playlist_removes_card() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SavePlaylist(Box::new(playlist(
            "pl1",
            "My Playlist",
        )))));
        let events = home.update_with(Cow::Owned(BrowserAction::UnsavePlaylist("pl1".to_owned())));

        assert_eq!(home.playlists.len(), 0);
        assert_eq!(events, vec![BrowserEvent::SavedPlaylistsUpdated]);
    }

    #[test]
    fn test_home_follow_artist_adds_card() {
        let mut home = HomeState::default();
        let events = home.update_with(Cow::Owned(BrowserAction::FollowArtist(artist_summary(
            "art1",
            "Cool Artist",
        ))));

        assert_eq!(home.artists.len(), 1);
        assert_eq!(home.artists.get(0).id(), "art1");
        assert_eq!(events, vec![BrowserEvent::SavedArtistsUpdated]);
    }

    #[test]
    fn test_home_follow_artist_is_idempotent() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::FollowArtist(artist_summary(
            "art1",
            "Cool Artist",
        ))));
        let events = home.update_with(Cow::Owned(BrowserAction::FollowArtist(artist_summary(
            "art1",
            "Cool Artist",
        ))));

        assert_eq!(home.artists.len(), 1);
        assert!(events.is_empty());
    }

    #[test]
    fn test_home_unfollow_artist_removes_card() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::FollowArtist(artist_summary(
            "art1",
            "Cool Artist",
        ))));
        let events = home.update_with(Cow::Owned(BrowserAction::UnfollowArtist("art1".to_owned())));

        assert_eq!(home.artists.len(), 0);
        assert_eq!(events, vec![BrowserEvent::SavedArtistsUpdated]);
    }

    fn saved_tracks_page(count: usize, offset: usize, total: usize) -> Page<Track> {
        let items = (0..count)
            .map(|i| make_track(&format!("track{}", offset + i)))
            .collect();
        Page::offset_paged(items, offset, total)
    }

    #[test]
    fn test_saved_tracks_total_known_from_first_page() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SetSavedTracks(Box::new(
            saved_tracks_page(50, 0, 1234),
        ))));

        // Only the first page is loaded, but the total is already known.
        assert_eq!(home.saved_tracks.partial_len(), 50);
        assert_eq!(home.saved_tracks_total, Some(1234));
    }

    #[test]
    fn test_liked_status_covers_tracks_outside_loaded_pages() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SetSavedTracks(Box::new(
            saved_tracks_page(50, 0, 1234),
        ))));
        assert!(!home.is_track_liked("old-like"));

        let events = home.update_with(Cow::Owned(BrowserAction::SetTracksLikedStatus(vec![
            ("old-like".to_string(), true),
            ("not-liked".to_string(), false),
        ])));

        assert!(home.is_track_liked("old-like"));
        assert!(!home.is_track_liked("not-liked"));
        assert!(matches!(events[..], [BrowserEvent::SavedTracksUpdated]));
    }

    #[test]
    fn test_liked_status_follows_likes_and_unlikes() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SetTracksLikedStatus(vec![(
            "a".to_string(),
            true,
        )])));

        home.update_with(Cow::Owned(BrowserAction::RemoveSavedTracks(vec![
            "a".to_string()
        ])));
        assert!(!home.is_track_liked("a"));

        home.update_with(Cow::Owned(BrowserAction::SaveTracks(vec![make_track("a")])));
        assert!(home.is_track_liked("a"));
    }

    #[test]
    fn test_liked_status_comes_with_fetched_tracks() {
        let mut home = HomeState::default();
        let mut liked = make_track("liked");
        liked.saved = Some(true);
        let mut not_liked = make_track("not-liked");
        not_liked.saved = Some(false);
        let unchecked = make_track("unchecked");

        let events = home.update_with(Cow::Owned(BrowserAction::SetArtistTopTracks(
            "artist".to_string(),
            vec![liked, not_liked, unchecked],
        )));

        assert!(home.is_track_liked("liked"));
        assert!(!home.is_track_liked("not-liked"));
        assert!(home.liked_status.contains_key("not-liked"));
        assert!(!home.liked_status.contains_key("unchecked"));
        assert!(matches!(events[..], [BrowserEvent::SavedTracksUpdated]));
    }

    #[test]
    fn test_late_liked_status_check_does_not_undo_a_like() {
        let mut home = HomeState::default();
        // The check for "a" went out, then the user liked it and unliked "b".
        home.update_with(Cow::Owned(BrowserAction::SaveTracks(vec![make_track("a")])));
        home.update_with(Cow::Owned(BrowserAction::RemoveSavedTracks(vec![
            "b".to_string()
        ])));

        let events = home.update_with(Cow::Owned(BrowserAction::SetTracksLikedStatus(vec![
            ("a".to_string(), false),
            ("b".to_string(), true),
        ])));

        assert!(home.is_track_liked("a"));
        assert!(!home.is_track_liked("b"));
        assert!(events.is_empty());
    }

    #[test]
    fn test_saved_tracks_page_overrides_an_earlier_check() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SetTracksLikedStatus(vec![
            ("track0".to_string(), false),
            ("track60".to_string(), false),
        ])));

        // Both were liked elsewhere since, and now show up in the saved list.
        let events = home.update_with(Cow::Owned(BrowserAction::SetSavedTracks(Box::new(
            saved_tracks_page(50, 0, 100),
        ))));
        assert!(home.is_track_liked("track0"));
        assert!(matches!(events[..], [BrowserEvent::SavedTracksUpdated]));

        home.update_with(Cow::Owned(BrowserAction::AppendSavedTracks(Box::new(
            saved_tracks_page(50, 50, 100),
        ))));
        assert!(home.is_track_liked("track60"));
    }

    #[test]
    fn test_saved_tracks_total_survives_appends() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SetSavedTracks(Box::new(
            saved_tracks_page(50, 0, 1234),
        ))));
        home.update_with(Cow::Owned(BrowserAction::AppendSavedTracks(Box::new(
            saved_tracks_page(50, 50, 1234),
        ))));

        assert_eq!(home.saved_tracks_total, Some(1234));
    }

    #[test]
    fn test_saved_tracks_total_tracks_likes_and_unlikes() {
        let mut home = HomeState::default();
        home.update_with(Cow::Owned(BrowserAction::SetSavedTracks(Box::new(
            saved_tracks_page(50, 0, 100),
        ))));

        home.update_with(Cow::Owned(BrowserAction::SaveTracks(vec![make_track(
            "new1",
        )])));
        assert_eq!(home.saved_tracks_total, Some(101));

        home.update_with(Cow::Owned(BrowserAction::RemoveSavedTracks(vec![
            "new1".to_owned()
        ])));
        assert_eq!(home.saved_tracks_total, Some(100));
    }
}
