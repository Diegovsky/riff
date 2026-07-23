use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use librespot::core::SpotifyUri;
use tokio::task;
use url::Url;

use crate::app::state::{LoginAction, PlaybackAction};
use crate::app::AppAction;
use crate::auth::TokenStore;
#[allow(clippy::module_inception)]
mod player;
pub use player::*;

#[derive(Debug, Clone)]
pub enum Command {
    Restore,
    InitLogin,
    CompleteLogin,
    RefreshToken,
    Logout,
    PlayerLoad {
        track: SpotifyUri,
        resume: bool,
    },
    PlayerResume,
    PlayerPause,
    PlayerStop,
    PlayerSeek(u32),
    PlayerSetVolume(f64),
    PlayerPreload(SpotifyUri),
    ReloadSettings,
    SetEqualizer {
        bands: [f64; 10],
    },
    SetMono {
        enabled: bool,
    },
    SetPan {
        pan: f64,
    },
    SetPitch {
        cents: f64,
    },
    // Re-query the account's explicit content filter (e.g. after a track was
    // rejected with ExplicitContentFiltered) and sync Riff's filter state.
    RecheckExplicitFilter,
    // Carries the result of an asynchronous explicit-filter re-check back into
    // the command loop so the player can update its cached state.
    ExplicitFilterRechecked {
        filter_enabled: bool,
        filter_locked: bool,
    },
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

    fn set_explicit_filter_locked(&self, locked: bool) {
        self.send(PlaybackAction::SetExplicitFilterLocked(locked).into())
    }

    fn set_skip_explicit(&self, skip: bool) {
        self.send(PlaybackAction::SetSkipExplicit(skip).into())
    }

    fn report_error(&self, error: SpotifyError) {
        self.send(match error {
            SpotifyError::NotPremium => LoginAction::SetNotPremium.into(),
            SpotifyError::LoginFailed => LoginAction::SetLoginFailure.into(),
            SpotifyError::LoggedOut => LoginAction::Logout.into(),
            _ => AppAction::ShowNotification(format!("{error}")),
        })
    }

    fn notify_playback_state(&self, position: u32) {
        self.send(PlaybackAction::SyncSeek(position).into())
    }

    fn preload_next_track(&self) {
        self.send(PlaybackAction::PreloadNext.into())
    }

    fn login_challenge_started(&self, url: Url) {
        self.send(LoginAction::OpenLoginUrl(url).into())
    }
}

#[tokio::main]
async fn player_main(
    player_settings: SpotifyPlayerSettings,
    appaction_sender: UnboundedSender<AppAction>,
    token_store: TokenStore,
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
    token_store: TokenStore,
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
