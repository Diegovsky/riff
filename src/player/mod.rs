use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use librespot::core::SpotifyUri;
use std::sync::Arc;
use tokio::task;
use url::Url;

use crate::app::state::{LoginAction, PlaybackAction};
use crate::app::AppAction;
#[allow(clippy::module_inception)]
mod player;
pub use player::*;

mod oauth2;

mod token_store;
pub use token_store::*;

#[derive(Debug, Clone)]
pub enum Command {
    Restore,
    InitLogin,
    CompleteLogin,
    RefreshToken,
    Logout,
    PlayerLoad { track: SpotifyUri, resume: bool },
    PlayerResume,
    PlayerPause,
    PlayerStop,
    PlayerSeek(u32),
    PlayerSetVolume(f64),
    PlayerPreload(SpotifyUri),
    ReloadSettings,
}

#[derive(Clone)]
pub(crate) struct AppPlayerDelegate {
    sender: UnboundedSender<AppAction>,
}

impl AppPlayerDelegate {
    fn new(sender: UnboundedSender<AppAction>) -> Self {
        Self { sender }
    }

    fn send(&self, action: AppAction) {
        self.sender.unbounded_send(action).unwrap();
    }

    fn end_of_track_reached(&self) {
        self.send(PlaybackAction::Next.into())
    }

    fn token_login_successful(&self, username: String) {
        self.send(LoginAction::SetLoginSuccess(username).into())
    }

    fn refresh_successful(&self) {
        self.send(LoginAction::TokenRefreshed.into())
    }

    fn report_error(&self, error: SpotifyError) {
        self.send(match error {
            SpotifyError::LoginFailed => LoginAction::SetLoginFailure.into(),
            SpotifyError::LoggedOut => LoginAction::Logout.into(),
            _ => AppAction::ShowNotification(format!("{error}")),
        })
    }

    fn notify_playback_state(&self, position: u32) {
        self.send(PlaybackAction::SyncSeek(position).into())
    }

    fn preload_next_track(&self) {
        self.send(PlaybackAction::Preload.into())
    }

    fn login_challenge_started(&self, url: Url) {
        self.send(LoginAction::OpenLoginUrl(url).into())
    }
}

#[tokio::main]
async fn player_main(
    player_settings: SpotifyPlayerSettings,
    appaction_sender: UnboundedSender<AppAction>,
    token_store: Arc<TokenStore>,
    sender: UnboundedSender<Command>,
    receiver: UnboundedReceiver<Command>,
) {
    task::spawn(async move {
        let delegate = AppPlayerDelegate::new(appaction_sender.clone());
        let player = SpotifyPlayer::new(player_settings, delegate, token_store, sender);
        player.start(receiver).await.unwrap();
    })
    .await
    .unwrap();
}

pub fn start_player_service(
    player_settings: SpotifyPlayerSettings,
    appaction_sender: UnboundedSender<AppAction>,
    token_store: Arc<TokenStore>,
) -> UnboundedSender<Command> {
    let (sender, receiver) = unbounded::<Command>();
    let sender_clone = sender.clone();
    std::thread::spawn(move || {
        player_main(
            player_settings,
            appaction_sender,
            token_store,
            sender_clone,
            receiver,
        )
    });
    sender
}
