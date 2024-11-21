use gettextrs::*;
use std::borrow::Cow;
use std::time::SystemTime;

use crate::app::credentials::Credentials;
use crate::app::models::PlaylistSummary;
use crate::app::state::{AppAction, AppEvent, UpdatableState};

#[derive(Clone, Debug)]
pub enum TryLoginAction {
    Reconnect(Credentials),
    NewLogin,
}

#[derive(Clone, Debug)]
pub enum SetLoginSuccessAction {
    Token(Credentials),
}

#[derive(Clone, Debug)]
pub enum LoginAction {
    ShowLogin,
    TryLogin(TryLoginAction),
    SetLoginSuccess(SetLoginSuccessAction),
    SetUserPlaylists(Vec<PlaylistSummary>),
    UpdateUserPlaylist(PlaylistSummary),
    PrependUserPlaylist(Vec<PlaylistSummary>),
    SetLoginFailure,
    RefreshToken(Credentials),
    SetRefreshedToken(Credentials),
    Logout,
}

impl From<LoginAction> for AppAction {
    fn from(login_action: LoginAction) -> Self {
        Self::LoginAction(login_action)
    }
}

#[derive(Clone, Debug)]
pub enum LoginStartedEvent {
    Reconnect(Credentials),
    NewLogin,
}

#[derive(Clone, Debug)]
pub enum LoginCompletedEvent {
    Token(Credentials),
}

#[derive(Clone, Debug)]
pub enum LoginEvent {
    LoginShown,
    LoginStarted(LoginStartedEvent),
    LoginCompleted(LoginCompletedEvent),
    UserPlaylistsLoaded,
    LoginFailed,
    FreshTokenRequested(Credentials),
    RefreshTokenCompleted(Credentials),
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
    // Playlists owned by the logged in user
    pub playlists: Vec<PlaylistSummary>,
}

impl UpdatableState for LoginState {
    type Action = LoginAction;
    type Event = AppEvent;

    // The login state has a lot of actions that just translate to events
    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        info!("update_with({:?})", action);
        match action.into_owned() {
            LoginAction::ShowLogin => vec![LoginEvent::LoginShown.into()],
            LoginAction::TryLogin(TryLoginAction::Reconnect(creds)) => {
                vec![LoginEvent::LoginStarted(LoginStartedEvent::Reconnect(creds)).into()]
            }
            LoginAction::SetLoginSuccess(SetLoginSuccessAction::Token(creds)) => {
                self.user = Some(creds.username.clone());
                vec![LoginEvent::LoginCompleted(LoginCompletedEvent::Token(creds)).into()]
            }
            LoginAction::SetLoginFailure => vec![LoginEvent::LoginFailed.into()],
            LoginAction::RefreshToken(creds) => vec![LoginEvent::FreshTokenRequested(creds).into()],
            LoginAction::SetRefreshedToken(creds) => {
                // translators: This notification is shown when, after some inactivity, the session is successfully restored. The user might have to repeat its last action.
                vec![
                    AppEvent::NotificationShown(gettext("Connection restored")),
                    LoginEvent::RefreshTokenCompleted(creds)
                    .into(),
                ]
            }
            LoginAction::Logout => {
                self.user = None;
                vec![LoginEvent::LogoutCompleted.into()]
            }
            LoginAction::SetUserPlaylists(playlists) => {
                self.playlists = playlists;
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::UpdateUserPlaylist(PlaylistSummary { id, title }) => {
                if let Some(p) = self.playlists.iter_mut().find(|p| p.id == id) {
                    p.title = title;
                }
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::PrependUserPlaylist(mut summaries) => {
                summaries.append(&mut self.playlists);
                self.playlists = summaries;
                vec![LoginEvent::UserPlaylistsLoaded.into()]
            }
            LoginAction::TryLogin(TryLoginAction::NewLogin) => {
                vec![LoginEvent::LoginStarted(LoginStartedEvent::NewLogin).into()]
            }
        }
    }
}
