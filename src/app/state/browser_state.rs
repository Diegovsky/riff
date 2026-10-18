use super::{
    AppAction, AppEvent, ArtistState, DetailsState, HomeState, PlaylistDetailsState, SearchState,
    UpdatableState, UserState,
};
use crate::app::components::{CardLayout, CardSize, SortOrder};
use crate::app::load;
use crate::app::models::*;
use riff_core::navigation::{EntryId, Navigator, Route, Transition};
use std::borrow::Cow;
use std::iter::Iterator;

#[derive(Clone, Debug)]
pub enum PaginationTarget {
    SavedAlbums,
    SavedPlaylists,
    SavedTracks,
    SavedArtists,
    ArtistReleases(String),
    UserPlaylists(String),
    PlaylistTracks(String),
    AlbumTracks(String),
    SearchScope,
}

// Actions that affect any "screen" that we push over time
#[derive(Clone, Debug)]
pub enum BrowserAction {
    SetNavigationHidden(bool),
    SetLibraryContent(Vec<Album>),
    PrependPlaylistsContent(Vec<Playlist>),
    AppendLibraryContent(Vec<Album>),
    SetPlaylistsContent(Vec<Playlist>),
    AppendPlaylistsContent(Vec<Playlist>),
    RemoveTracksFromPlaylist(String, Vec<String>),
    AddTracksToPlaylist(String, Vec<Track>),
    RemovePlaylist(String),
    SetAlbumInfo(Box<Album>),
    SetAlbumTracks(String, Box<Page<Track>>),
    SetAlbumLikedStatus(String, bool),
    AppendAlbumTracks(String, Box<Page<Track>>),
    SetPlaylistDetails(Box<Playlist>, Box<Page<Track>>),
    UpdatePlaylistName(PlaylistSummary),
    UpdatePlaylistArt(String, ImageSet),
    UpdatePlaylistInfo(String, Option<String>, Option<bool>),
    AppendPlaylistTracks(String, Box<Page<Track>>),
    Search(String),
    /// Used to tell the currently loaded Details screen to play a song
    PlaySong(String),
    SetSearchResults(Box<SearchResults>),
    /// Change the active search filter (`None` = combined results).
    SetSearchFilter(Option<SearchType>),
    /// Replace the results of the scoped search view (artists/albums/playlists/tracks).
    SetSearchScopeResults(SearchType, String, Box<SearchResults>),
    /// Append a page of results to the scoped search view.
    AppendSearchScopeResults(SearchType, String, Box<SearchResults>),
    SetArtistInfo(Box<Artist>),
    SetArtistReleases(String, Vec<Album>),
    SetArtistTopTracks(String, Vec<Track>),
    SetArtistFollowedStatus(String, bool),
    AppendArtistReleases(String, Vec<Album>),
    Navigate(Route),
    NavigationBack,
    NavigationForward,
    NavigationGoTo(EntryId),
    NavigationLeave(Route),
    SaveScroll(EntryId, u32),
    SaveAlbum(Box<Album>),
    UnsaveAlbum(String),
    SetUserInfo(Box<User>),
    SetUserPlaylists(String, Vec<Playlist>),
    AppendUserPlaylists(String, Vec<Playlist>),
    SetSavedTracks(Box<Page<Track>>),
    AppendSavedTracks(Box<Page<Track>>),
    SaveTracks(Vec<Track>),
    RemoveSavedTracks(Vec<String>),
    SetTracksLikedStatus(Vec<(String, bool)>),
    ClearTracksLikedStatus,
    SetSavedArtists(Vec<Artist>, Option<String>),
    AppendSavedArtists(Vec<Artist>, Option<String>),
    FollowArtist(Artist),
    UnfollowArtist(String),
    SavePlaylist(Box<Playlist>),
    UnsavePlaylist(String),
    ConsumeNextPage(PaginationTarget),
    PageRequestFailed(PaginationTarget, usize),
    ChangeCardLayout(CardLayout),
    ChangeCardSize(CardSize),
    ChangeSortOrder(String, SortOrder),
    /// Refresh navigation panel and details pin UI without reloading saved playlists from Spotify.
    NotifyPinnedPlaylistsUpdated,
}

impl From<BrowserAction> for AppAction {
    fn from(browser_action: BrowserAction) -> Self {
        Self::BrowserAction(browser_action)
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum BrowserEvent {
    NavigationHidden(bool),
    NavigationChanged(Transition),
    NavigationHistoryChanged,
    LibraryUpdated,
    SavedPlaylistsUpdated,
    /// Emitted when local pinned playlist IDs change (no Spotify fetch).
    PinnedPlaylistsUpdated,
    CardLayoutChanged(CardLayout),
    CardSizeChanged(CardSize),
    SortOrderChanged(String, SortOrder),
    AlbumDetailsLoaded(String),
    AlbumTracksAppended(String),
    PlaylistDetailsLoaded(String),
    PlaylistTracksAppended(String),
    PlaylistTracksRemoved(String),
    SearchUpdated,
    SearchResultsUpdated,
    /// The active search filter changed.
    SearchFilterChanged,
    /// The scoped search view's results changed.
    SearchScopeUpdated(SearchType),
    ArtistDetailsUpdated(String),
    AlbumSaved(String),
    /// Notifies a DetailsModel that a song is requested
    SongPlaybackRequested(String),
    AlbumUnsaved(String),
    PlaylistSaved(String),
    PlaylistUnsaved(String),
    UserDetailsUpdated(String),
    SavedTracksUpdated,
    SavedArtistsUpdated,
}

impl From<BrowserEvent> for AppEvent {
    fn from(browser_event: BrowserEvent) -> Self {
        Self::BrowserEvent(browser_event)
    }
}

pub enum BrowserScreen {
    Home(Box<HomeState>),
    AlbumDetails(Box<DetailsState>),
    Search(Box<SearchState>),
    Artist(Box<ArtistState>),
    PlaylistDetails(Box<PlaylistDetailsState>),
    User(Box<UserState>),
}

impl BrowserScreen {
    fn from_route(route: &Route) -> Self {
        match route {
            Route::Home(_) => BrowserScreen::Home(Default::default()),
            Route::Album(id) => {
                BrowserScreen::AlbumDetails(Box::new(DetailsState::new(id.clone())))
            }
            Route::Search => BrowserScreen::Search(Default::default()),
            Route::Artist(id) => BrowserScreen::Artist(Box::new(ArtistState::new(id.clone()))),
            Route::Playlist(id) => {
                BrowserScreen::PlaylistDetails(Box::new(PlaylistDetailsState::new(id.clone())))
            }
            Route::User(id) => BrowserScreen::User(Box::new(UserState::new(id.clone()))),
        }
    }

    fn shows(&self, route: &Route) -> bool {
        match (self, route) {
            (Self::Home(_), Route::Home(_)) | (Self::Search(_), Route::Search) => true,
            (Self::AlbumDetails(s), Route::Album(id)) => &s.id == id,
            (Self::Artist(s), Route::Artist(id)) => &s.id == id,
            (Self::PlaylistDetails(s), Route::Playlist(id)) => &s.id == id,
            (Self::User(s), Route::User(id)) => &s.id == id,
            _ => false,
        }
    }

    // Each screen has a state that can be updated with a BrowserAction
    fn state(&mut self) -> &mut dyn UpdatableState<Action = BrowserAction, Event = BrowserEvent> {
        match self {
            Self::Home(state) => &mut **state,
            Self::AlbumDetails(state) => &mut **state,
            Self::Search(state) => &mut **state,
            Self::Artist(state) => &mut **state,
            Self::PlaylistDetails(state) => &mut **state,
            Self::User(state) => &mut **state,
        }
    }
}

pub struct BrowserState {
    navigation_hidden: bool,
    navigator: Navigator,
    screens: Vec<BrowserScreen>,
}

macro_rules! extract_state {
    ($e:expr, $p:pat if $guard:expr => $i:ident) => {
        extract_state_full!($e, $p if $guard => $i)
    };
    ($e:expr, $p:pat => $i:ident) => {
        extract_state_full!($e, $p if true => $i)
    };
}

macro_rules! extract_state_full {
    ($e:expr, $p:pat if $guard:expr => $i:ident) => {{
        $e.screens.iter().find_map(|screen| match screen {
            $p if $guard => Some(&**$i),
            _ => None,
        })
    }};
}

impl BrowserState {
    pub fn new() -> Self {
        Self {
            navigation_hidden: false,
            navigator: Navigator::default(),
            screens: vec![BrowserScreen::Home(Default::default())],
        }
    }

    pub fn current_screen(&self) -> &Route {
        self.navigator.current_route()
    }

    pub fn navigator(&self) -> &Navigator {
        &self.navigator
    }

    pub fn can_go_back(&self) -> bool {
        self.navigator.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.navigator.can_go_forward()
    }

    pub fn home_state(&self) -> Option<&HomeState> {
        extract_state!(self, BrowserScreen::Home(s) => s)
    }

    pub fn home_state_mut(&mut self) -> Option<&mut HomeState> {
        self.screens.iter_mut().find_map(|screen| match screen {
            BrowserScreen::Home(s) => Some(&mut **s),
            _ => None,
        })
    }

    pub fn details_state(&self, id: &str) -> Option<&DetailsState> {
        extract_state!(self, BrowserScreen::AlbumDetails(state) if state.id == id => state)
    }

    pub fn search_state(&self) -> Option<&SearchState> {
        extract_state!(self, BrowserScreen::Search(s) => s)
    }

    pub fn artist_state(&self, id: &str) -> Option<&ArtistState> {
        extract_state!(self, BrowserScreen::Artist(state) if state.id == id => state)
    }

    pub fn playlist_details_state(&self, id: &str) -> Option<&PlaylistDetailsState> {
        extract_state!(self, BrowserScreen::PlaylistDetails(state) if state.id == id => state)
    }

    pub fn user_state(&self, id: &str) -> Option<&UserState> {
        extract_state!(self, BrowserScreen::User(state) if state.id == id => state)
    }

    fn apply(&mut self, transition: Option<Transition>) -> Vec<BrowserEvent> {
        let Some(transition) = transition else {
            return vec![];
        };
        self.screens
            .retain(|screen| !transition.evicted.iter().any(|route| screen.shows(route)));
        if !self
            .screens
            .iter()
            .any(|screen| screen.shows(&transition.to))
        {
            self.screens.push(BrowserScreen::from_route(&transition.to));
        }
        load::bump_epoch();
        vec![BrowserEvent::NavigationChanged(transition)]
    }

    fn record_title(&mut self, action: &BrowserAction) -> Vec<BrowserEvent> {
        let (route, title) = match action {
            BrowserAction::SetAlbumInfo(album) => {
                (Route::Album(album.rri.id.clone()), &album.title)
            }
            BrowserAction::SetArtistInfo(artist) => {
                (Route::Artist(artist.rri.id.clone()), &artist.name)
            }
            BrowserAction::SetPlaylistDetails(playlist, _) => {
                (Route::Playlist(playlist.rri.id.clone()), &playlist.title)
            }
            BrowserAction::UpdatePlaylistName(PlaylistSummary { id, title }) => {
                (Route::Playlist(id.clone()), title)
            }
            BrowserAction::SetUserInfo(user) => {
                (Route::User(user.rri.id.clone()), &user.display_name)
            }
            _ => return vec![],
        };
        if self.navigator.set_title(&route, title) {
            vec![BrowserEvent::NavigationHistoryChanged]
        } else {
            vec![]
        }
    }
}

impl UpdatableState for BrowserState {
    type Action = BrowserAction;
    type Event = BrowserEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        let action_ref = action.as_ref();

        match action_ref {
            BrowserAction::SetNavigationHidden(navigation_hidden) => {
                self.navigation_hidden = *navigation_hidden;
                vec![BrowserEvent::NavigationHidden(*navigation_hidden)]
            }
            BrowserAction::Search(_) => {
                let transition = self.navigator.navigate(Route::Search);
                let mut events = self.apply(transition);
                let search = self
                    .screens
                    .iter_mut()
                    .find(|screen| screen.shows(&Route::Search));
                if let Some(search) = search {
                    events.append(&mut search.state().update_with(action));
                }
                events
            }
            BrowserAction::Navigate(route) => {
                let transition = self.navigator.navigate(route.clone());
                self.apply(transition)
            }
            BrowserAction::NavigationBack if self.navigator.can_go_back() => {
                let transition = self.navigator.back();
                self.apply(transition)
            }
            BrowserAction::NavigationBack if self.navigation_hidden => {
                self.navigation_hidden = false;
                vec![BrowserEvent::NavigationHidden(false)]
            }
            BrowserAction::NavigationForward => {
                let transition = self.navigator.forward();
                self.apply(transition)
            }
            BrowserAction::NavigationGoTo(id) => {
                let transition = self.navigator.go_to(*id);
                self.apply(transition)
            }
            BrowserAction::NavigationLeave(route) => {
                let transition = self.navigator.leave_if_current(route);
                self.apply(transition)
            }
            BrowserAction::SaveScroll(id, scroll) => {
                self.navigator.save_scroll(*id, *scroll);
                vec![]
            }
            BrowserAction::ChangeCardLayout(layout) => {
                vec![BrowserEvent::CardLayoutChanged(*layout)]
            }
            BrowserAction::ChangeCardSize(size) => {
                vec![BrowserEvent::CardSizeChanged(*size)]
            }
            BrowserAction::ChangeSortOrder(page, order) => {
                vec![BrowserEvent::SortOrderChanged(page.clone(), *order)]
            }
            BrowserAction::NotifyPinnedPlaylistsUpdated => {
                vec![BrowserEvent::PinnedPlaylistsUpdated]
            }
            _ => {
                let mut events = self.record_title(action_ref);
                events.extend(
                    self.screens
                        .iter_mut()
                        .flat_map(|s| s.state().update_with(Cow::Borrowed(action_ref))),
                );
                events
            }
        }
    }
}

#[cfg(test)]
pub mod tests {

    use super::*;
    use riff_core::navigation::{Direction, HomePage};

    const LIBRARY: Route = Route::Home(HomePage::Library);

    fn artist(id: &str) -> Route {
        Route::Artist(id.to_string())
    }

    fn album(id: &str) -> Route {
        Route::Album(id.to_string())
    }

    fn navigate(state: &mut BrowserState, route: Route) -> Vec<BrowserEvent> {
        state.update_with(Cow::Owned(BrowserAction::Navigate(route)))
    }

    fn back(state: &mut BrowserState) -> Vec<BrowserEvent> {
        state.update_with(Cow::Owned(BrowserAction::NavigationBack))
    }

    fn forward(state: &mut BrowserState) -> Vec<BrowserEvent> {
        state.update_with(Cow::Owned(BrowserAction::NavigationForward))
    }

    /// Going back must bump the epoch too, or work queued for the screen the
    /// user left keeps competing with the one they moved to.
    #[test]
    fn every_navigation_advances_the_epoch() {
        let mut state = BrowserState::new();

        let before_push = load::current_epoch();
        navigate(&mut state, artist("some_id"));
        let after_push = load::current_epoch();
        assert!(after_push > before_push, "going somewhere new");

        back(&mut state);
        let after_back = load::current_epoch();
        assert!(after_back > after_push, "going back");

        forward(&mut state);
        assert!(load::current_epoch() > after_back, "going forward");
    }

    /// Re-requesting the current screen must not demote its own work.
    #[test]
    fn renavigating_to_the_current_screen_is_not_a_navigation() {
        let mut state = BrowserState::new();
        navigate(&mut state, artist("current"));

        let before = load::current_epoch();
        assert!(navigate(&mut state, artist("current")).is_empty());
        assert_eq!(load::current_epoch(), before);
    }

    #[test]
    fn navigating_creates_the_screen_state() {
        let mut state = BrowserState::new();
        assert_eq!(*state.current_screen(), LIBRARY);

        let events = navigate(&mut state, artist("some_id"));

        assert_eq!(state.current_screen(), &artist("some_id"));
        assert!(state.artist_state("some_id").is_some());
        assert!(matches!(
            events.as_slice(),
            [BrowserEvent::NavigationChanged(Transition {
                direction: Direction::Forward,
                ..
            })]
        ));
    }

    #[test]
    fn back_and_forward() {
        let mut state = BrowserState::new();
        navigate(&mut state, artist("a"));
        navigate(&mut state, album("b"));

        back(&mut state);
        assert_eq!(state.current_screen(), &artist("a"));
        assert!(state.can_go_forward());
        assert!(state.details_state("b").is_some());

        forward(&mut state);
        assert_eq!(state.current_screen(), &album("b"));
        assert!(forward(&mut state).is_empty());
    }

    #[test]
    fn back_at_the_start_does_nothing() {
        let mut state = BrowserState::new();
        assert_eq!(back(&mut state), vec![]);
        assert_eq!(*state.current_screen(), LIBRARY);
    }

    #[test]
    fn back_at_the_start_shows_hidden_navigation() {
        let mut state = BrowserState::new();
        state.update_with(Cow::Owned(BrowserAction::SetNavigationHidden(true)));
        assert_eq!(
            back(&mut state),
            vec![BrowserEvent::NavigationHidden(false)]
        );
    }

    #[test]
    fn home_pages_keep_history() {
        let mut state = BrowserState::new();
        navigate(&mut state, album("a"));
        navigate(&mut state, Route::Home(HomePage::SavedArtists));

        back(&mut state);
        assert_eq!(state.current_screen(), &album("a"));
    }

    #[test]
    fn search_has_a_back_button() {
        let mut state = BrowserState::new();
        state.update_with(Cow::Owned(BrowserAction::Search("q".to_string())));
        assert_eq!(state.current_screen(), &Route::Search);
        assert!(state.can_go_back());
        assert!(state.search_state().is_some());
    }

    #[test]
    fn leaving_a_missing_item_only_when_current() {
        let mut state = BrowserState::new();
        navigate(&mut state, album("missing"));
        navigate(&mut state, artist("b"));

        let events =
            state.update_with(Cow::Owned(BrowserAction::NavigationLeave(album("missing"))));
        assert!(events.is_empty());
        assert_eq!(state.current_screen(), &artist("b"));
    }

    #[test]
    fn far_screens_drop_their_state() {
        let mut state = BrowserState::new();
        for i in 0..=Navigator::DEFAULT_LIVE_LIMIT {
            navigate(&mut state, album(&i.to_string()));
        }
        assert!(state.details_state("0").is_none());
        assert!(state.details_state("1").is_some());
        assert!(state.home_state().is_some());
    }

    #[test]
    fn notify_pinned_playlists_updated_emits_event() {
        let mut state = BrowserState::new();
        let events = state.update_with(Cow::Owned(BrowserAction::NotifyPinnedPlaylistsUpdated));
        assert_eq!(events, vec![BrowserEvent::PinnedPlaylistsUpdated]);
    }
}
