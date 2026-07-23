use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;

use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;

use librespot::playback::mixer::softmixer::SoftMixer;
use librespot::playback::mixer::{Mixer, MixerConfig};

use librespot::playback::audio_backend;
use librespot::playback::audio_backend::Sink;
use librespot::playback::config::{
    AudioFormat, Bitrate, NormalisationMethod, NormalisationType, PlayerConfig, VolumeCtrl,
};
use librespot::playback::player::{Player, PlayerEvent, PlayerEventChannel};

use crate::app::models::RepeatMode;
use crate::audio_engine::{
    CaptureSink, EqController, EqProcessor, MixController, MixProcessor, MonoController,
    MonoProcessor, PanController, PanProcessor, PitchController, PitchProcessor, ProcessorChain,
};
use crate::player::AppPlayerDelegate;

use crate::auth::{AuthcodeChallenge, OAuthError, RiffOauthClient, TokenStore};

use super::Command;
use crate::app::credentials;
use crate::settings::RiffSettings;
use std::env;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

#[derive(Debug)]
pub enum SpotifyError {
    LoginFailed,
    NotPremium,
    LoggedOut,
    PlayerNotReady,
    TechnicalError,
}

impl Error for SpotifyError {}

impl fmt::Display for SpotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoginFailed => write!(f, "Login failed!"),
            Self::NotPremium => write!(f, "A Spotify Premium subscription is required."),
            Self::LoggedOut => write!(f, "You are logged out!"),
            Self::PlayerNotReady => write!(f, "Player is not responding."),
            Self::TechnicalError => {
                write!(f, "A technical error occured. Check your connectivity.")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioBackend {
    GStreamer(String),
    PulseAudio,
    Alsa(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeCurveType {
    Log,
    Linear,
    Cubic,
}

impl Default for VolumeCurveType {
    fn default() -> Self {
        Self::Log
    }
}

#[derive(Debug, Clone)]
pub struct SpotifyPlayerSettings {
    pub bitrate: Bitrate,
    pub backend: AudioBackend,
    pub gapless: bool,
    pub ap_port: Option<u16>,

    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub volume: f64,

    // Local user preference: skip tracks marked as explicit. Applied via the
    // playback state, not librespot, so it never requires a player reload.
    pub skip_explicit: bool,

    // Volume curve
    pub volume_curve: VolumeCurveType,

    // Normalization
    pub normalisation: bool,
    pub normalisation_type: NormalisationType,
    pub normalisation_method: NormalisationMethod,
    pub normalisation_pregain_db: f64,
    pub normalisation_threshold_dbfs: f64,
    pub normalisation_attack_ms: f64,
    pub normalisation_release_ms: f64,
    pub normalisation_knee_db: f64,

    // Audio format
    pub audio_format: AudioFormat,

    // Mono audio
    pub mono_audio: bool,

    // Stereo pan / balance (-1.0 = full left, 0.0 = center, 1.0 = full right).
    // Always enabled; centered has no effect.
    pub pan: f64,

    // Pitch shift in cents (1 cent = 1/100 semitone). 0.0 = no shift.
    pub pitch_cents: f64,

    // Equalizer. Active whenever any band is non-zero; flat = passthrough.
    pub eq_bands: [f64; 10],
}

impl Default for SpotifyPlayerSettings {
    fn default() -> Self {
        Self {
            volume: 0.7,
            repeat: RepeatMode::None,
            shuffle: false,

            skip_explicit: false,

            bitrate: Bitrate::Bitrate160,
            gapless: true,
            backend: AudioBackend::PulseAudio,
            ap_port: None,

            volume_curve: VolumeCurveType::Log,

            normalisation: false,
            normalisation_type: NormalisationType::Auto,
            normalisation_method: NormalisationMethod::Dynamic,
            normalisation_pregain_db: 0.0,
            normalisation_threshold_dbfs: -2.0,
            normalisation_attack_ms: 5.0,
            normalisation_release_ms: 100.0,
            normalisation_knee_db: 5.0,

            audio_format: AudioFormat::default(),

            mono_audio: false,

            pan: 0.0,

            pitch_cents: 0.0,

            eq_bands: [0.0; 10],
        }
    }
}

impl SpotifyPlayerSettings {
    /// Whether a change from `self` to `other` requires recreating the librespot
    /// player (which interrupts playback). Equalizer, mono, pan, and pitch
    /// settings are excluded: they are applied live via their controllers and
    /// never require a reload.
    pub fn requires_reload(&self, other: &Self) -> bool {
        /// Epsilon for comparing normalisation parameters. Values come from
        /// GtkSpinRow adjustments (step = 0.5), so a threshold well below the
        /// smallest meaningful step avoids spurious reloads from FP rounding.
        const EPS: f64 = 1.0e-9;

        #[inline]
        fn f64_changed(a: f64, b: f64) -> bool {
            (a - b).abs() > EPS
        }

        self.bitrate != other.bitrate
            || self.backend != other.backend
            || self.gapless != other.gapless
            || self.ap_port != other.ap_port
            || self.volume_curve != other.volume_curve
            || self.normalisation != other.normalisation
            || self.normalisation_type != other.normalisation_type
            || self.normalisation_method != other.normalisation_method
            || f64_changed(
                self.normalisation_pregain_db,
                other.normalisation_pregain_db,
            )
            || f64_changed(
                self.normalisation_threshold_dbfs,
                other.normalisation_threshold_dbfs,
            )
            || f64_changed(self.normalisation_attack_ms, other.normalisation_attack_ms)
            || f64_changed(
                self.normalisation_release_ms,
                other.normalisation_release_ms,
            )
            || f64_changed(self.normalisation_knee_db, other.normalisation_knee_db)
            || self.audio_format != other.audio_format
    }
}

pub struct SpotifyPlayer {
    settings: SpotifyPlayerSettings,
    player: Option<Arc<Player>>,
    mixer: Option<Box<dyn Mixer>>,
    session: Option<Session>,

    // Shared equalizer configuration, updated live without recreating the player.
    eq_controller: EqController,

    // Shared mono audio setting, updated live without recreating the player.
    mono_controller: MonoController,

    // Shared pan/balance configuration, updated live without recreating the player.
    pan_controller: PanController,

    // Shared pitch-shift setting (stub), updated live without recreating the player.
    pitch_controller: PitchController,

    // Shared mixer setting (stub), updated live without recreating the player.
    mix_controller: MixController,

    // Auth related stuff
    oauth_client: Arc<RiffOauthClient>,
    auth_challenge: Option<AuthcodeChallenge>,
    command_sender: UnboundedSender<Command>,

    // Cached "explicit filter locked" state for the account, so we avoid
    // re-querying the profile once we know the filter is locked for the
    // session (a locked filter cannot be unlocked without changing account
    // settings, and forces skipping so explicit tracks are never attempted).
    explicit_filter_locked: bool,

    // Receives feedback from commands or various events in the player
    delegate: AppPlayerDelegate,
}

impl SpotifyPlayer {
    pub fn new(
        settings: SpotifyPlayerSettings,
        delegate: AppPlayerDelegate,
        token_store: TokenStore,
        command_sender: UnboundedSender<Command>,
    ) -> Self {
        let eq_controller = EqController::new(settings.eq_bands);
        let mono_controller = MonoController::new(settings.mono_audio);
        let pan_controller = PanController::new(settings.pan);
        let pitch_controller = PitchController::new(settings.pitch_cents);
        let mix_controller = MixController::new(false);
        Self {
            settings,
            mixer: None,
            player: None,
            session: None,
            eq_controller,
            mono_controller,
            pan_controller,
            pitch_controller,
            mix_controller,
            oauth_client: Arc::new(RiffOauthClient::new(token_store)),
            auth_challenge: None,
            command_sender,
            explicit_filter_locked: false,
            delegate,
        }
    }

    async fn handle_and_notify(&mut self, action: Command) {
        match self.handle(action).await {
            Ok(_) => {}
            Err(e) => self.delegate.report_error(e),
        }
    }

    fn get_player(&self) -> Result<&Arc<Player>, SpotifyError> {
        self.player.as_ref().ok_or(SpotifyError::PlayerNotReady)
    }

    fn get_player_mut(&mut self) -> Result<&mut Arc<Player>, SpotifyError> {
        self.player.as_mut().ok_or(SpotifyError::PlayerNotReady)
    }

    async fn handle(&mut self, action: Command) -> Result<(), SpotifyError> {
        match action {
            Command::PlayerSetVolume(volume) => {
                if let Some(mixer) = self.mixer.as_mut() {
                    mixer_set_volume(&mut **mixer, volume);
                }
                Ok(())
            }
            Command::SetEqualizer { bands } => {
                // Live update: no player/session recreation, no playback interruption.
                self.settings.eq_bands = bands;
                self.eq_controller.update(bands);
                Ok(())
            }
            Command::SetMono { enabled } => {
                // Live update: no player/session recreation, no playback interruption.
                self.settings.mono_audio = enabled;
                self.mono_controller.update(enabled);
                Ok(())
            }
            Command::SetPan { pan } => {
                // Live update: no player/session recreation, no playback interruption.
                self.settings.pan = pan;
                self.pan_controller.update(pan);
                Ok(())
            }
            Command::SetPitch { cents } => {
                // Live update: no player/session recreation, no playback interruption.
                self.settings.pitch_cents = cents;
                self.pitch_controller.update(cents);
                Ok(())
            }
            Command::PlayerResume => {
                self.get_player()?.play();
                Ok(())
            }
            Command::PlayerPause => {
                self.get_player()?.pause();
                Ok(())
            }
            Command::PlayerStop => {
                self.get_player()?.stop();
                Ok(())
            }
            Command::PlayerSeek(position) => {
                self.get_player()?.seek(position);
                Ok(())
            }
            Command::PlayerLoad { track, resume } => {
                debug!("Player: playing track {track}");
                self.get_player_mut()?.load(track, resume, 0);
                Ok(())
            }
            Command::PlayerPreload(track) => {
                self.get_player_mut()?.preload(track);
                Ok(())
            }
            Command::RefreshToken => {
                let session = self.session.as_ref().ok_or(SpotifyError::PlayerNotReady)?;
                let token = self
                    .oauth_client
                    .get_valid_token()
                    .await
                    .map_err(|_| SpotifyError::LoginFailed)?;
                let credentials = Credentials::with_access_token(token.access_token.clone());
                session
                    .connect(credentials, true)
                    .await
                    .map_err(|_| SpotifyError::LoginFailed)?;
                self.delegate.refresh_successful();
                Ok(())
            }
            Command::Logout => {
                self.oauth_client.clear_credentials().await;
                if let Some(session) = self.session.take() {
                    session.shutdown();
                }
                let _ = self.player.take();
                Ok(())
            }
            Command::Restore => {
                info!("Attempting to restore session from stored credentials");
                let credentials = self.oauth_client.get_valid_token().await.map_err(|e| {
                    error!("Failed to get valid token during restore: {e:?}");
                    match e {
                        OAuthError::LoggedOut => SpotifyError::LoggedOut,
                        _ => SpotifyError::LoginFailed,
                    }
                })?;

                info!(
                    "Restoring session (token expires at {:?})",
                    credentials.token_expiry_time
                );
                self.initial_login(credentials).await
            }
            Command::InitLogin => {
                let auth_url = match self.auth_challenge.as_ref() {
                    Some(challenge) => challenge.auth_url.clone(),
                    None => {
                        let cmd = self.command_sender.clone();
                        let challenge = self
                            .oauth_client
                            .spawn_authcode_listener(move || {
                                cmd.unbounded_send(Command::CompleteLogin).unwrap();
                            })
                            .await
                            .map_err(|_| SpotifyError::LoginFailed)?;
                        let auth_url = challenge.auth_url.clone();
                        self.auth_challenge = Some(challenge);
                        auth_url
                    }
                };
                self.delegate.login_challenge_started(auth_url);
                Ok(())
            }
            Command::CompleteLogin => {
                let Some(challenge) = self.auth_challenge.take() else {
                    error!("CompleteLogin called but no auth challenge was pending");
                    return Err(SpotifyError::LoginFailed);
                };

                info!("Exchanging auth code for token");
                let credentials = self
                    .oauth_client
                    .exchange_authcode(challenge)
                    .await
                    .map_err(|e| {
                        error!("Auth code exchange failed: {e:?}");
                        SpotifyError::LoginFailed
                    })?;

                info!(
                    "Login with OAuth2 (token expires at {:?})",
                    credentials.token_expiry_time
                );
                self.initial_login(credentials).await
            }
            Command::ReloadSettings => {
                let settings = RiffSettings::new_from_gsettings().unwrap_or_default();
                self.settings = settings.player_settings;

                // Clear the mixer so it gets recreated with updated volume curve/dB range
                self.mixer.take();

                let session = self.session.take().ok_or(SpotifyError::PlayerNotReady)?;
                let new_player = self.create_player(session);
                tokio::task::spawn(player_setup_delegate(
                    new_player.get_player_event_channel(),
                    self.delegate.clone(),
                    self.command_sender.clone(),
                ));
                self.player.replace(new_player);

                Ok(())
            }
            Command::RecheckExplicitFilter => {
                // Already locked for this session: skipping is forced and
                // explicit tracks are never attempted, so no need to re-query.
                if self.explicit_filter_locked {
                    return Ok(());
                }

                // Run the profile lookup off the command loop so it never
                // delays playback commands (e.g. loading the next track). The
                // result comes back as ExplicitFilterRechecked.
                let oauth_client = Arc::clone(&self.oauth_client);
                let command_sender = self.command_sender.clone();
                tokio::task::spawn(async move {
                    let token = match oauth_client.get_valid_token().await {
                        Ok(t) => t,
                        Err(e) => {
                            warn!("Explicit filter re-check: could not get token: {e:?}");
                            return;
                        }
                    };
                    match crate::api::check_user_profile(&token.access_token).await {
                        Ok(profile) => {
                            let _ =
                                command_sender.unbounded_send(Command::ExplicitFilterRechecked {
                                    filter_enabled: profile.explicit_filter_enabled,
                                    filter_locked: profile.explicit_filter_locked,
                                });
                        }
                        Err(e) => {
                            warn!("Failed to re-check explicit content filter: {e:?}");
                        }
                    }
                });
                Ok(())
            }
            Command::ExplicitFilterRechecked {
                filter_enabled,
                filter_locked,
            } => {
                if filter_locked && !self.explicit_filter_locked {
                    info!("Explicit content filter is now locked; syncing Riff's filter");
                }
                self.explicit_filter_locked = filter_locked;
                self.delegate.set_explicit_filter_locked(filter_locked);
                // When the account's filter is enabled (even if not locked),
                // Spotify's servers reject explicit tracks. Enable client-side
                // skipping so we never attempt to load them.
                if filter_enabled {
                    self.delegate.set_skip_explicit(true);
                }
                Ok(())
            }
        }
    }

    async fn initial_login(
        &mut self,
        credentials: credentials::Credentials,
    ) -> Result<(), SpotifyError> {
        // Check if the account is premium before connecting to librespot.
        // librespot will crash the process for free accounts, so we must
        // catch this early and report a graceful error instead.
        let profile = match crate::api::check_user_profile(&credentials.access_token).await {
            Ok(p) => p,
            Err(e) => {
                error!("User profile check failed: {e:?}");
                return Err(SpotifyError::LoginFailed);
            }
        };

        if !profile.is_premium {
            warn!("Account is not premium, aborting login");
            return Err(SpotifyError::NotPremium);
        }

        // Sync the explicit content filter from the user's Spotify account.
        // When filter_enabled is true, Spotify's servers will reject explicit
        // tracks (returning Unavailable to librespot). We must enable
        // client-side skipping so Riff never attempts to load those tracks in
        // the first place - avoiding a cascade of Unavailable rejections that
        // can disrupt playback of non-explicit tracks too.
        //
        // A locked filter (e.g. a family plan parental control) additionally
        // prevents the user from disabling the skip in Riff's settings.
        if profile.explicit_filter_enabled {
            info!("Spotify account has explicit content filter enabled");
            self.delegate.set_skip_explicit(true);
        }
        if profile.explicit_filter_locked {
            info!("Spotify account explicit content filter is locked");
        }
        self.explicit_filter_locked = profile.explicit_filter_locked;
        self.delegate
            .set_explicit_filter_locked(profile.explicit_filter_locked);

        // Only persist credentials to the keyring after confirming premium status.
        // This prevents non-premium accounts from being saved and retried on next launch.
        self.oauth_client.save_credentials(&credentials).await;

        let creds = Credentials::with_access_token(&credentials.access_token);
        info!(
            "Creating librespot session (ap_port: {:?})",
            self.settings.ap_port
        );
        let new_session = match create_session(&creds, self.settings.ap_port).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create librespot session: {e:?}");
                return Err(e);
            }
        };
        let username = new_session.username();
        info!("Session created successfully for user: {username}");

        // Disable librespot's built-in explicit content filtering. When the
        // account has filter_enabled=true, Spotify sets the session attribute
        // "filter-explicit-content" to "1". This causes the audio key server
        // to deny decryption keys for ALL tracks (not just explicit ones),
        // breaking playback entirely. We override it to "0" so librespot can
        // load any track, and enforce the explicit filter ourselves at the
        // playback-state level where we only skip tracks actually marked
        // explicit.
        new_session.set_user_attribute("filter-explicit-content", "0");

        let oauth_client = Arc::clone(&self.oauth_client);
        let session = new_session.clone();
        tokio::task::spawn(async move {
            // Scheduling loop: wait until the token is near expiry, refresh,
            // reconnect, and repeat. The refresh itself long-polls transient
            // failures inside refresh_token_at_expiry(), so an error surfaces
            // here only when it is fatal (no stored token, or the refresh
            // token was rejected and credentials were cleared). In that case
            // there is nothing left to retry; exit and let a subsequent login
            // spawn a fresh loop.
            loop {
                match oauth_client.refresh_token_at_expiry().await {
                    Ok(token) => {
                        _ = session
                            .connect(Credentials::with_access_token(token.access_token), true)
                            .await;
                    }
                    Err(e) => {
                        warn!("Token refresh loop stopping: {e}");
                        break;
                    }
                }
            }
        });

        let new_player = self.create_player(new_session.clone());
        tokio::task::spawn(player_setup_delegate(
            new_player.get_player_event_channel(),
            self.delegate.clone(),
            self.command_sender.clone(),
        ));

        self.player.replace(new_player);
        self.session.replace(new_session);
        self.delegate.token_login_successful(username);

        Ok(())
    }

    fn create_player(&mut self, session: Session) -> Arc<Player> {
        let backend = self.settings.backend.clone();
        let audio_format = self.settings.audio_format;

        // Convert attack/release from milliseconds to coefficients
        let normalisation_attack_cf = librespot::playback::player::duration_to_coefficient(
            std::time::Duration::from_secs_f64(self.settings.normalisation_attack_ms / 1000.0),
        );
        let normalisation_release_cf = librespot::playback::player::duration_to_coefficient(
            std::time::Duration::from_secs_f64(self.settings.normalisation_release_ms / 1000.0),
        );

        let player_config = PlayerConfig {
            gapless: self.settings.gapless,
            bitrate: self.settings.bitrate,
            normalisation: self.settings.normalisation,
            normalisation_type: self.settings.normalisation_type,
            normalisation_method: self.settings.normalisation_method,
            normalisation_pregain_db: self.settings.normalisation_pregain_db,
            normalisation_threshold_dbfs: self.settings.normalisation_threshold_dbfs,
            normalisation_attack_cf,
            normalisation_release_cf,
            normalisation_knee_db: self.settings.normalisation_knee_db,
            ..Default::default()
        };
        info!("bitrate: {:?}", &player_config.bitrate);
        info!(
            "volume curve: {:?}, dB range: {:.1}",
            self.settings.volume_curve,
            VolumeCtrl::DEFAULT_DB_RANGE
        );
        if player_config.normalisation {
            info!(
                "normalisation: type={:?}, method={:?}, pregain={:.1}dB",
                player_config.normalisation_type,
                player_config.normalisation_method,
                player_config.normalisation_pregain_db
            );
        }

        let volume = self.settings.volume;
        let volume_curve = self.settings.volume_curve;
        let soft_volume = self
            .mixer
            .get_or_insert_with(|| {
                let volume_ctrl = match volume_curve {
                    VolumeCurveType::Log => VolumeCtrl::Log(VolumeCtrl::DEFAULT_DB_RANGE),
                    VolumeCurveType::Linear => VolumeCtrl::Linear,
                    VolumeCurveType::Cubic => VolumeCtrl::Cubic(VolumeCtrl::DEFAULT_DB_RANGE),
                };
                let mut mix = Box::new(
                    SoftMixer::open(MixerConfig {
                        volume_ctrl,
                        ..Default::default()
                    })
                    .expect("Failed to create soft mixer"),
                );
                mixer_set_volume(&mut *mix, volume);
                mix
            })
            .get_soft_volume();

        let eq_controller = self.eq_controller.clone();
        let mono_controller = self.mono_controller.clone();
        let pan_controller = self.pan_controller.clone();
        let pitch_controller = self.pitch_controller.clone();
        let mix_controller = self.mix_controller.clone();

        Player::new(player_config, session, soft_volume, move || {
            let sink: Box<dyn Sink> = match backend {
                AudioBackend::GStreamer(pipeline) => {
                    let backend = audio_backend::find(Some("gstreamer".to_string())).unwrap();
                    backend(Some(pipeline), audio_format)
                }
                AudioBackend::PulseAudio => {
                    info!("using pulseaudio");
                    env::set_var("PULSE_PROP_application.name", "Riff");
                    let backend = audio_backend::find(Some("pulseaudio".to_string())).unwrap();
                    backend(None, audio_format)
                }
                AudioBackend::Alsa(device) => {
                    info!("using alsa ({})", &device);
                    let backend = audio_backend::find(Some("alsa".to_string())).unwrap();
                    backend(Some(device), audio_format)
                }
            };

            // Route decoded audio through the audio engine pipeline before it
            // reaches the backend. The chain runs in a fixed order; each stage
            // passes audio through untouched while disabled and applies live
            // updates via its controller otherwise.
            let chain = ProcessorChain::new()
                .with(Box::new(EqProcessor::new(eq_controller)))
                .with(Box::new(MonoProcessor::new(mono_controller)))
                .with(Box::new(PanProcessor::new(pan_controller)))
                .with(Box::new(PitchProcessor::new(pitch_controller)))
                .with(Box::new(MixProcessor::new(mix_controller)));

            CaptureSink::wrap(sink, chain)
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
    let root = glib::user_cache_dir().join("riff").join("librespot");
    let cache = Cache::new(
        Some(root.join("credentials")),
        Some(root.join("volume")),
        Some(root.join("audio")),
        None,
    )
    .map_err(|e| dbg!(e))
    .ok();
    debug!("Connecting librespot session (ap_port={:?})", ap_port);
    let session = Session::new(session_config, cache);
    match session.connect(credentials.clone(), true).await {
        Ok(_) => {
            info!("librespot session connected successfully");
            Ok(session)
        }
        Err(err) => {
            error!(
                "librespot session connect failed (ap_port={:?}): {}",
                ap_port, err
            );
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
    delegate: AppPlayerDelegate,
    command_sender: UnboundedSender<Command>,
) {
    while let Some(event) = channel.recv().await {
        match event {
            PlayerEvent::EndOfTrack { .. } => {
                delegate.end_of_track_reached();
            }
            PlayerEvent::Unavailable { track_id, .. } => {
                warn!(
                    "Track unavailable (possibly explicit-filtered): {:?}, skipping",
                    track_id
                );
                // Gracefully skip the track that could not be played.
                delegate.end_of_track_reached();
                // The account's explicit filter may have changed since login
                // (e.g. a family plan parental control was just set). Re-query
                // it so Riff's filter state stays in sync.
                let _ = command_sender.unbounded_send(Command::RecheckExplicitFilter);
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

/// Maps a 0.0–1.0 volume slider value to the mixer's u16 volume.
///
/// The VolumeCtrl curve (configured in create_player) determines the dB mapping.
/// The curve's db_range parameter (derived from volume_min_db/volume_max_db settings)
/// controls how many dB of dynamic range the slider spans.
fn mixer_set_volume(mixer: &mut dyn Mixer, volume: f64) {
    mixer.set_volume((VolumeCtrl::MAX_VOLUME as f64 * volume) as u16);
}
