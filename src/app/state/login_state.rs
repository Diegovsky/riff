use std::borrow::Cow;
use std::collections::HashSet;
use url::Url;

use crate::app::models::PlaylistSummary;
use crate::app::state::{AppAction, AppEvent, UpdatableState};

#[derive(Clone, Debug)]
pub enum TryLoginAction {
    Restore,
    InitLogin,
}

#[derive(Clone, Debug)]
pub enum LoginAction {
    OpenLoginUrl(Url),
    TryLogin(TryLoginAction),
    SetLoginSuccess(String),
    SetUserPlaylists(Vec<PlaylistSummary>, HashSet<String>),
    UpdateUserPlaylist(PlaylistSummary),
    PrependUserPlaylist(Vec<PlaylistSummary>),
    PrependOwnedPlaylist(PlaylistSummary),
    RemoveUserPlaylist(String),
    SetLoginFailure,
    SetNotPremium,
    TokenRefreshed,
    Logout,
}

impl From<LoginAction> for AppAction {
    fn from(login_action: LoginAction) -> Self {
        Self::LoginAction(login_action)
    }
}

#[derive(Clone, Debug)]
pub enum LoginStartedEvent {
    Restore,
    InitLogin,
    OpenUrl(Url),
}

#[derive(Clone, Debug)]
pub enum LoginEvent {
    LoginStarted(LoginStartedEvent),
    LoginCompleted,
    UserPlaylistsLoaded,
    LoginFailed,
    NotPremium,
    RefreshTokenCompleted,
    LogoutCompleted,
}

impl From<LoginEvent> for AppEvent {
    fn from(login_event: LoginEvent) -> Self {
        Self::LoginEvent(login_event)
    }
}

#[derive(Default)]
pub struct LoginState {
    // Username
    pub user: Option<String>,
    // Playlists saved by the logged in user: their own and those they follow
    pub playlists: Vec<PlaylistSummary>,
    // Playlist IDs for O(1) saved checks
    pub playlist_ids: HashSet<String>,
    // IDs of the saved playlists the user owns
    pub owned_playlist_ids: HashSet<String>,
}

impl LoginState {
    pub fn owned_playlists(&self) -> Vec<PlaylistSummary> {
        self.playlists
            .iter()
            .filter(|p| self.owned_playlist_ids.contains(&p.id))
            .cloned()
            .collect()
    }
}

impl UpdatableState for LoginState {
    type Action = LoginAction;
    type Event = AppEvent;

    // The login state has a lot of actions that just translate to events
    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        info!("update_with({:?})", action);
        match action.into_owned() {
            LoginAction::OpenLoginUrl(url) => {
                vec![LoginEvent::LoginStarted(LoginStartedEvent::OpenUrl(url)).into()]
            }
            LoginAction::TryLogin(TryLoginAction::Restore) => {
                vec![LoginEvent::LoginStarted(LoginStartedEvent::Restore).into()]
            }
            LoginAction::SetLoginSuccess(username) => {
                self.user = Some(username);
                vec![LoginEvent::LoginCompleted.into()]
            }
            LoginAction::SetLoginFailure => vec![LoginEvent::LoginFailed.into()],
            LoginAction::SetNotPremium => vec![LoginEvent::NotPremium.into()],
            LoginAction::TokenRefreshed => {
                vec![LoginEvent::RefreshTokenCompleted.into()]
            }
            LoginAction::Logout => {
                self.user = None;
                vec![LoginEvent::LogoutCompleted.into()]
            }
            LoginAction::SetUserPlaylists(playlists, owned) => {
                self.playlist_ids = playlists.iter().map(|p| p.id.clone()).collect();
                self.playlists = playlists;
                self.owned_playlist_ids = owned;
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::UpdateUserPlaylist(PlaylistSummary { id, title }) => {
                if let Some(p) = self.playlists.iter_mut().find(|p| p.id == id) {
                    p.title = title;
                }
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::PrependUserPlaylist(mut summaries) => {
                for s in &summaries {
                    self.playlist_ids.insert(s.id.clone());
                }
                summaries.append(&mut self.playlists);
                self.playlists = summaries;
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::PrependOwnedPlaylist(summary) => {
                self.playlist_ids.insert(summary.id.clone());
                self.owned_playlist_ids.insert(summary.id.clone());
                self.playlists.insert(0, summary);
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::RemoveUserPlaylist(id) => {
                self.playlists.retain(|p| p.id != id);
                self.playlist_ids.remove(&id);
                self.owned_playlist_ids.remove(&id);
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::TryLogin(TryLoginAction::InitLogin) => {
                vec![LoginEvent::LoginStarted(LoginStartedEvent::InitLogin).into()]
            }
        }
    }
}
