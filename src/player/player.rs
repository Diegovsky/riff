use futures::channel::mpsc::UnboundedReceiver;
use futures::stream::StreamExt;

use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;

use librespot::playback::mixer::softmixer::SoftMixer;
use librespot::playback::mixer::{Mixer, MixerConfig};

use librespot::playback::audio_backend;
use librespot::playback::config::{AudioFormat, Bitrate, PlayerConfig, VolumeCtrl};
use librespot::playback::player::{Player, PlayerEvent, PlayerEventChannel};

use super::oauth2::SpotOauthClient;
use super::Command;
use crate::app::credentials;
use crate::settings::SpotSettings;
use std::env;
use std::error::Error;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub enum SpotifyError {
    LoginFailed,
    PlayerNotReady,
    TechnicalError,
}

impl Error for SpotifyError {}

impl fmt::Display for SpotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoginFailed => write!(f, "Login failed!"),
            Self::PlayerNotReady => write!(f, "Player is not responding."),
            Self::TechnicalError => {
                write!(f, "A technical error occured. Check your connectivity.")
            }
        }
    }
}

pub trait SpotifyPlayerDelegate {
    fn end_of_track_reached(&self);
    fn token_login_successful(&self, credentials: credentials::Credentials);
    fn refresh_successful(&self, credentials: credentials::Credentials);
    fn report_error(&self, error: SpotifyError);
    fn notify_playback_state(&self, position: u32);
    fn preload_next_track(&self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioBackend {
    GStreamer(String),
    PulseAudio,
    Alsa(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyPlayerSettings {
    pub bitrate: Bitrate,
    pub backend: AudioBackend,
    pub gapless: bool,
    pub ap_port: Option<u16>,
}

impl Default for SpotifyPlayerSettings {
    fn default() -> Self {
        Self {
            bitrate: Bitrate::Bitrate160,
            gapless: true,
            backend: AudioBackend::PulseAudio,
            ap_port: None,
        }
    }
}

pub struct SpotifyPlayer {
    settings: SpotifyPlayerSettings,
    player: Option<Arc<Player>>,
    mixer: Option<Box<dyn Mixer>>,
    session: Option<Session>,
    oauth_client: SpotOauthClient,
    delegate: Rc<dyn SpotifyPlayerDelegate>,
}

impl SpotifyPlayer {
    pub fn new(settings: SpotifyPlayerSettings, delegate: Rc<dyn SpotifyPlayerDelegate>) -> Self {
        Self {
            settings,
            mixer: None,
            player: None,
            session: None,
            oauth_client: SpotOauthClient::new(),
            delegate,
        }
    }

    async fn handle_and_notify(&mut self, action: Command) {
        match self.handle(action).await {
            Ok(_) => {}
            Err(e) => self.delegate.report_error(e),
        }
    }

    async fn handle(&mut self, action: Command) -> Result<(), SpotifyError> {
        match action {
            Command::PlayerSetVolume(volume) => {
                if let Some(mixer) = self.mixer.as_mut() {
                    mixer.set_volume((VolumeCtrl::MAX_VOLUME as f64 * volume) as u16);
                }
                Ok(())
            }
            Command::PlayerResume => {
                self.player
                    .as_ref()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .play();
                Ok(())
            }
            Command::PlayerPause => {
                self.player
                    .as_ref()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .pause();
                Ok(())
            }
            Command::PlayerStop => {
                self.player
                    .as_ref()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .stop();
                Ok(())
            }
            Command::PlayerSeek(position) => {
                self.player
                    .as_ref()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .seek(position);
                Ok(())
            }
            Command::PlayerLoad { track, resume } => {
                self.player
                    .as_mut()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .load(track, resume, 0);
                Ok(())
            }
            Command::PlayerPreload(track) => {
                self.player
                    .as_mut()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .preload(track);
                Ok(())
            }
            Command::RefreshToken(credentials) => {
                let session = self.session.as_ref().ok_or(SpotifyError::PlayerNotReady)?;
                let token = self
                    .oauth_client
                    .refresh_token(credentials)
                    .await
                    .map_err(|_| SpotifyError::LoginFailed)?;
                let credentials = Credentials::with_access_token(token.access_token.clone());
                session
                    .connect(credentials, true)
                    .await
                    .map_err(|_| SpotifyError::LoginFailed)?;
                self.delegate.refresh_successful(token);
                Ok(())
            }
            Command::Logout => {
                self.session
                    .take()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .shutdown();
                let _ = self.player.take();
                Ok(())
            }
            Command::Reconnect(mut credentials) => {
                info!("Login with token, username {}", &credentials.username);
                if credentials.token_expired() {
                    credentials = self
                        .oauth_client
                        .refresh_token(credentials)
                        .await
                        .map_err(|_| SpotifyError::LoginFailed)?;
                }
                let creds = Credentials::with_access_token(credentials.access_token.clone());
                let new_session = create_session(&creds, self.settings.ap_port).await?;
                self.delegate.token_login_successful(credentials);

                let new_player = self.create_player(new_session.clone());
                tokio::task::spawn_local(player_setup_delegate(
                    new_player.get_player_event_channel(),
                    Rc::clone(&self.delegate),
                ));
                self.player.replace(new_player);
                self.session.replace(new_session);

                Ok(())
            }
            Command::NewLogin => {
                let mut credentials = self
                    .oauth_client
                    .get_token()
                    .await
                    .map_err(|_| SpotifyError::LoginFailed)?;
                info!("Login with OAuth2");
                let new_session = create_session(
                    &Credentials::with_access_token(&credentials.access_token),
                    self.settings.ap_port,
                )
                .await?;
                credentials.username = new_session.username();
                self.delegate.token_login_successful(credentials);

                let new_player = self.create_player(new_session.clone());
                tokio::task::spawn_local(player_setup_delegate(
                    new_player.get_player_event_channel(),
                    Rc::clone(&self.delegate),
                ));
                self.player.replace(new_player);
                self.session.replace(new_session);

                Ok(())
            }
            Command::ReloadSettings => {
                let settings = SpotSettings::new_from_gsettings().unwrap_or_default();
                self.settings = settings.player_settings;

                let session = self.session.take().ok_or(SpotifyError::PlayerNotReady)?;
                let new_player = self.create_player(session);
                tokio::task::spawn_local(player_setup_delegate(
                    new_player.get_player_event_channel(),
                    Rc::clone(&self.delegate),
                ));
                self.player.replace(new_player);

                Ok(())
            }
        }
    }

    fn create_player(&mut self, session: Session) -> Arc<Player> {
        let backend = self.settings.backend.clone();

        let player_config = PlayerConfig {
            gapless: self.settings.gapless,
            bitrate: self.settings.bitrate,
            ..Default::default()
        };
        info!("bitrate: {:?}", &player_config.bitrate);

        let soft_volume = self
            .mixer
            .get_or_insert_with(|| {
                let mix = Box::new(SoftMixer::open(MixerConfig {
                    // This value feels reasonable to me. Feel free to change it
                    volume_ctrl: VolumeCtrl::Log(VolumeCtrl::DEFAULT_DB_RANGE / 2.0),
                    ..Default::default()
                }));
                // TODO: Should read volume from somewhere instead of hard coding.
                // Sets volume to 100%
                mix.set_volume(VolumeCtrl::MAX_VOLUME);
                mix
            })
            .get_soft_volume();
        Player::new(player_config, session, soft_volume, move || match backend {
            AudioBackend::GStreamer(pipeline) => {
                let backend = audio_backend::find(Some("gstreamer".to_string())).unwrap();
                backend(Some(pipeline), AudioFormat::default())
            }
            AudioBackend::PulseAudio => {
                info!("using pulseaudio");
                env::set_var("PULSE_PROP_application.name", "Spot");
                let backend = audio_backend::find(Some("pulseaudio".to_string())).unwrap();
                backend(None, AudioFormat::default())
            }
            AudioBackend::Alsa(device) => {
                info!("using alsa ({})", &device);
                let backend = audio_backend::find(Some("alsa".to_string())).unwrap();
                backend(Some(device), AudioFormat::default())
            }
        })
    }

    pub async fn start(self, receiver: UnboundedReceiver<Command>) -> Result<(), ()> {
        receiver
            .fold(self, |mut player, action| async {
                player.handle_and_notify(action).await;
                player
            })
            .await;
        Ok(())
    }
}

const KNOWN_AP_PORTS: [Option<u16>; 4] = [None, Some(80), Some(443), Some(4070)];

async fn create_session_with_port(
    credentials: &Credentials,
    ap_port: Option<u16>,
) -> Result<Session, SpotifyError> {
    let session_config = SessionConfig {
        ap_port,
        ..Default::default()
    };
    let root = glib::user_cache_dir().join("spot").join("librespot");
    let cache = Cache::new(
        Some(root.join("credentials")),
        Some(root.join("volume")),
        Some(root.join("audio")),
        None,
    )
    .map_err(|e| dbg!(e))
    .ok();
    let session = Session::new(session_config, cache);
    match session.connect(credentials.clone(), true).await {
        Ok(_) => Ok(session),
        Err(err) => {
            warn!("Login failure: {}", err);
            Err(SpotifyError::LoginFailed)
        }
    }
}

async fn create_session(
    credentials: &Credentials,
    ap_port: Option<u16>,
) -> Result<Session, SpotifyError> {
    match ap_port {
        Some(_) => create_session_with_port(credentials, ap_port).await,
        None => {
            let mut ports_to_try = KNOWN_AP_PORTS.iter();
            loop {
                if let Some(next_port) = ports_to_try.next() {
                    let res = create_session_with_port(credentials, *next_port).await;
                    match res {
                        Err(SpotifyError::TechnicalError) => continue,
                        _ => break res,
                    }
                } else {
                    break Err(SpotifyError::TechnicalError);
                }
            }
        }
    }
}

async fn player_setup_delegate(
    mut channel: PlayerEventChannel,
    delegate: Rc<dyn SpotifyPlayerDelegate>,
) {
    while let Some(event) = channel.recv().await {
        match event {
            PlayerEvent::EndOfTrack { .. } => {
                delegate.end_of_track_reached();
            }
            PlayerEvent::Playing { position_ms, .. } => {
                delegate.notify_playback_state(position_ms);
            }
            PlayerEvent::TimeToPreloadNextTrack { .. } => {
                debug!("Requesting next track to be preloaded...");
                delegate.preload_next_track();
            }
            _ => {}
        }
    }
}
