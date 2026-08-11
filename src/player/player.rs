use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;

use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::SessionConfig;
use librespot::core::error::ErrorKind;
use librespot::core::session::Session;
use librespot::core::SpotifyUri;

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

use crate::auth::{AuthcodeChallenge, OAuthError, RiffOauthClient, TokenStore, SESSION_CLIENT_ID};

use super::Command;
use crate::app::credentials;
use crate::settings::RiffSettings;
use std::env;
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum SpotifyError {
    LoginFailed,
    NotPremium,
    LoggedOut,
    PlayerNotReady,
    TechnicalError,
    // Every track fails to decrypt (librespot "audio key error"): the signature
    // of Spotify's PlayPlay DRM, which blocks playback entirely.
    PlaybackDrmBlocked,
    // Many tracks failed to load in a row on an account that has already played
    // this session, so likely transient rather than a DRM block.
    PlaybackTemporarilyUnavailable,
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
            Self::PlaybackDrmBlocked => write!(
                f,
                "Playback is not available for this account. Spotify's PlayPlay DRM \
                 is proprietary and cannot be implemented outside the official client."
            ),
            Self::PlaybackTemporarilyUnavailable => {
                write!(f, "Playback is temporarily unavailable.")
            }
        }
    }
}

// Consecutive load failures before we stop playback, halting a runaway churn
// through the queue. If nothing has played since login, this run also signals
// PlayPlay DRM. High enough that one removed album won't trip it.
// See https://github.com/librespot-org/librespot/issues/1649.
const CONSECUTIVE_UNAVAILABLE_STOP_THRESHOLD: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioBackend {
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
    session_auth_challenge: Option<AuthcodeChallenge>,
    command_sender: UnboundedSender<Command>,

    // librespot sessions are single-use: once the connection to the access
    // point drops, the session is invalidated forever and a brand new
    // Session + Player must be built. The fields below let us detect that,
    // resume playback where we left off, and throttle reconnect attempts.

    // Playback state preserved across a rebuild:

    // The last track loaded into the player, if any.
    current_track: Option<SpotifyUri>,
    // Whether the user paused playback; a rebuilt session must not start
    // blasting music when the user had paused.
    is_paused: bool,
    // Whether the current track's playback was actually interrupted (a load
    // failed on a dead session) and must be reloaded once a fresh session is
    // in place. A dead session alone does NOT interrupt playback as track
    // audio streams from the CDN over separate connections and buffered audio
    // keeps playing. In these cases reconnects must leave the player alone.
    track_needs_reload: bool,
    // Last known playback position, updated live by the player event
    // listener (player_setup_delegate) from another task.
    last_position_ms: Arc<AtomicU32>,

    // Reconnect backoff and the session-health watchdog:

    // Exponential backoff for watchdog-driven reconnects, so a real network
    // outage doesn't make us hammer Spotify's access points.
    reconnect_attempts: u32,
    next_reconnect_at: Option<Instant>,
    // The session-health watchdog task is spawned once per service lifetime
    // on first successful login.
    watchdog_spawned: bool,

    // Background task handles:

    // Handle to the background token-refresh loop. Kept so it can be aborted
    // before a fresh session is created, preventing duplicate refresh loops
    // from accumulating across reconnects.
    token_refresh_task: Option<tokio::task::JoinHandle<()>>,

    // Flags shared with the player event task:

    // Tells the player event handler to suppress skip actions while a
    // reconnection is in progress. Without this, Unavailable events from
    // librespot create a tight loop that can starve the system when the
    // network is down.
    connection_lost: Arc<AtomicBool>,
    // Set by the player event listener when a track finishes (EndOfTrack)
    // while the connection is down. Once the session is rebuilt, the player
    // advances to the next track instead of sitting in silence.
    advance_after_reconnect: Arc<AtomicBool>,
    // Consecutive load failures; see CONSECUTIVE_UNAVAILABLE_STOP_THRESHOLD.
    // Reset when a track plays and when the threshold fires.
    consecutive_load_failures: Arc<AtomicU32>,
    // Whether any track has played since login. If so, load failures are
    // individual unavailable tracks, not a DRM block. Reset on login/logout,
    // not on reconnect.
    has_played_since_login: Arc<AtomicBool>,
    // Logged-in account id, used to persist the verified-playable marker.
    current_user_id: Option<String>,

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
            session_auth_challenge: None,
            command_sender,

            // Playback state preserved across a rebuild.
            current_track: None,
            is_paused: false,
            track_needs_reload: false,
            last_position_ms: Arc::new(AtomicU32::new(0)),

            // Reconnect backoff and the session-health watchdog.
            reconnect_attempts: 0,
            next_reconnect_at: None,
            watchdog_spawned: false,

            // Background task handles.
            token_refresh_task: None,

            // Flags shared with the player event task.
            connection_lost: Arc::new(AtomicBool::new(false)),
            advance_after_reconnect: Arc::new(AtomicBool::new(false)),
            consecutive_load_failures: Arc::new(AtomicU32::new(0)),
            has_played_since_login: Arc::new(AtomicBool::new(false)),
            current_user_id: None,

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
                self.ensure_session_alive().await?;
                self.is_paused = false;
                self.get_player()?.play();
                Ok(())
            }
            Command::PlayerPause => {
                self.is_paused = true;
                self.get_player()?.pause();
                Ok(())
            }
            Command::PlayerStop => {
                self.current_track = None;
                self.is_paused = false;
                self.track_needs_reload = false;
                self.advance_after_reconnect.store(false, Ordering::Relaxed);
                self.get_player()?.stop();
                Ok(())
            }
            Command::PlayerSeek(position) => {
                // Record the target first: if a rebuild has to reload the
                // interrupted track, it picks it up directly at this position
                // (a seek issued while the reloaded track is still loading
                // would restart the load).
                self.last_position_ms.store(position, Ordering::Relaxed);
                self.ensure_session_alive().await?;
                self.get_player()?.seek(position);
                Ok(())
            }
            Command::PlayerLoad { track, resume } => {
                debug!("Player: playing track {track}");
                self.ensure_session_alive().await?;
                self.current_track = Some(track.clone());
                self.is_paused = !resume;
                // Loading a new track supersedes any pending reload or deferred
                // advancement of the previous one.
                self.track_needs_reload = false;
                self.advance_after_reconnect.store(false, Ordering::Relaxed);
                self.last_position_ms.store(0, Ordering::Relaxed);
                self.get_player_mut()?.load(track, resume, 0);
                Ok(())
            }
            Command::PlayerPreload(track) => {
                // No reconnect for preloads: they are opportunistic, and a
                // dead session is handled when the track is actually loaded.
                self.get_player_mut()?.preload(track);
                Ok(())
            }
            Command::RefreshToken => {
                self.session.as_ref().ok_or(SpotifyError::PlayerNotReady)?;
                // Refresh the OAuth token (used by the web API). The librespot
                // session keeps its already-authenticated connection, so it
                // only needs the new token if it has to be rebuilt. Sessions
                // cannot be reconnected in place (see ensure_session_alive).
                self.oauth_client
                    .get_valid_token()
                    .await
                    .map_err(|_| SpotifyError::LoginFailed)?;
                self.ensure_session_alive().await?;
                self.delegate.refresh_successful();
                Ok(())
            }
            Command::ReconnectSession => {
                // Requested by the session watchdog, the token refresh loop or
                // TrackUnavailable when the session looks dead.
                self.try_background_reconnect().await
            }
            Command::TrackUnavailable => {
                // The player failed to load a track. A dead session makes
                // every load fail this way; skipping in that state would
                // cascade through the whole queue and end in silence, so
                // reconnect and retry instead. Only a healthy session's
                // verdict is trusted as "this track really can't be played".
                if self.session_needs_rebuild() {
                    warn!("Track load failed because the session died: reconnecting");
                    // Playback of the current track really was interrupted;
                    // the rebuild must reload it at the last known position.
                    self.track_needs_reload = true;
                    return self.try_background_reconnect().await;
                }
                self.handle_unavailable_track();
                Ok(())
            }
            #[cfg(debug_assertions)]
            Command::DevSimulateTrackUnavailable => {
                // Run the real handler without the session health check.
                warn!("[dev] Simulating an unavailable track failure");
                self.handle_unavailable_track();
                Ok(())
            }
            Command::MarkPlaybackVerified => {
                // A track played, so this account is not DRM-blocked; persist it once.
                if let Some(user_id) = self.current_user_id.as_deref() {
                    if !user_id.is_empty() && crate::settings::drm_verified_user() != user_id {
                        info!("Recording account as verified playable");
                        crate::settings::set_drm_verified_user(user_id);
                    }
                }
                Ok(())
            }
            Command::Logout => {
                self.oauth_client.clear_credentials().await;
                if let Some(handle) = self.token_refresh_task.take() {
                    handle.abort();
                }
                if let Some(session) = self.session.take() {
                    session.shutdown();
                }
                let _ = self.player.take();
                self.current_track = None;
                self.is_paused = false;
                self.track_needs_reload = false;
                self.reconnect_attempts = 0;
                self.next_reconnect_at = None;
                self.connection_lost.store(false, Ordering::Relaxed);
                self.advance_after_reconnect.store(false, Ordering::Relaxed);
                self.consecutive_load_failures.store(0, Ordering::Relaxed);
                self.has_played_since_login.store(false, Ordering::Relaxed);
                self.delegate.set_connection_lost(false);
                Ok(())
            }
            Command::Restore => {
                info!("Attempting to restore session from stored credentials");
                let credentials =
                    self.oauth_client
                        .get_valid_token()
                        .await
                        .map_err(|e| match e {
                            // Having no stored credentials is the expected first-run
                            // state, not a failure: just show the login screen.
                            OAuthError::LoggedOut => {
                                info!("No stored credentials to restore; showing login");
                                SpotifyError::LoggedOut
                            }
                            e => {
                                error!("Failed to get valid token during restore: {e:?}");
                                SpotifyError::LoginFailed
                            }
                        })?;

                info!(
                    "Restoring session (token expires at {:?})",
                    credentials.token_expiry_time
                );
                if !session_cache_usable() {
                    info!("No usable session credentials on disk; showing login");
                    return Err(SpotifyError::LoggedOut);
                }
                self.begin_login(credentials).await
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
                self.begin_login(credentials).await
            }
            Command::CompleteSessionLogin => {
                let Some(challenge) = self.session_auth_challenge.take() else {
                    error!("CompleteSessionLogin called but no session auth challenge was pending");
                    return Err(SpotifyError::LoginFailed);
                };

                info!("Exchanging playback auth code for a session token");
                let access_token = self
                    .oauth_client
                    .exchange_session_authcode(challenge)
                    .await
                    .map_err(|e| {
                        error!("Session auth code exchange failed: {e:?}");
                        SpotifyError::LoginFailed
                    })?;

                let session =
                    connect_session_from_token(&access_token, self.settings.ap_port).await?;
                note_session_minted();
                self.finish_login(session);
                Ok(())
            }
            Command::ReloadSettings => {
                let settings = RiffSettings::new_from_gsettings().unwrap_or_default();
                self.settings = settings.player_settings;

                // Clear the mixer so it gets recreated with updated volume curve/dB range
                self.mixer.take();

                // Keep the session: it stays valid across player reloads
                // (only the player is recreated around it).
                let session = self
                    .session
                    .as_ref()
                    .ok_or(SpotifyError::PlayerNotReady)?
                    .clone();
                self.build_and_store_player(session);

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
            #[cfg(debug_assertions)]
            Command::DevKillPlayer => {
                warn!("[dev] Killing librespot player (session left intact)");
                // Drop the player so the audio engine is torn down. The
                // session is still alive but now has no player; the health
                // watchdog detects the missing player via session_needs_rebuild
                // and rebuilds it (matching how a genuinely dead player
                // recovers in production).
                if let Some(player) = self.player.take() {
                    player.stop();
                }
                Ok(())
            }
            #[cfg(debug_assertions)]
            Command::DevKillSession => {
                warn!("[dev] Killing librespot session (player left intact)");
                // Shut the session down so it becomes invalid. It is left in
                // place (not taken) so session_needs_rebuild sees it as dead
                // and the watchdog reconnects, exercising the real reconnect
                // path.
                if let Some(session) = self.session.as_ref() {
                    session.shutdown();
                }
                self.connection_lost.store(true, Ordering::Relaxed);
                self.delegate.set_connection_lost(true);
                Ok(())
            }
            #[cfg(debug_assertions)]
            Command::DevExpireToken => {
                warn!("[dev] Expiring cached OAuth token and forcing a refresh");
                match self.oauth_client.dev_expire_and_refresh().await {
                    Ok(_) => {
                        info!("[dev] Token refresh succeeded");
                        self.delegate.refresh_successful();
                        Ok(())
                    }
                    Err(OAuthError::LoggedOut) => Err(SpotifyError::LoggedOut),
                    Err(e) => {
                        warn!("[dev] Token refresh failed: {e}");
                        Err(SpotifyError::LoginFailed)
                    }
                }
            }
            #[cfg(debug_assertions)]
            Command::DevResetDrmVerification => {
                warn!("[dev] Clearing verified-playable marker and re-arming DRM detection");
                // Forget the persisted known-good account.
                crate::settings::clear_drm_verified_user();
                // Re-arm detection for the current session.
                self.has_played_since_login.store(false, Ordering::Relaxed);
                self.consecutive_load_failures.store(0, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    async fn begin_login(
        &mut self,
        credentials: credentials::Credentials,
    ) -> Result<(), SpotifyError> {
        // Fresh login: reset the failure run so a previous account can't trip
        // a DRM block. has_played_since_login is seeded per-account below.
        self.consecutive_load_failures.store(0, Ordering::Relaxed);

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

        // Seed DRM detection from the persisted per-account marker: a known-good
        // account starts as "already played" so its unavailable tracks aren't
        // mistaken for a DRM block.
        let known_good =
            !profile.user_id.is_empty() && crate::settings::drm_verified_user() == profile.user_id;
        self.current_user_id = Some(profile.user_id.clone());
        self.has_played_since_login
            .store(known_good, Ordering::Relaxed);
        if known_good {
            debug!("Account previously verified as playable; DRM detection disabled");
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

        if session_cache_usable() {
            info!(
                "Reusing cached session credentials (ap_port: {:?})",
                self.settings.ap_port
            );
            let session = connect_session_from_cache(self.settings.ap_port).await?;
            self.finish_login(session);
            Ok(())
        } else {
            info!("No usable session credentials; starting playback login");
            self.start_session_login().await
        }
    }

    async fn start_session_login(&mut self) -> Result<(), SpotifyError> {
        let cmd = self.command_sender.clone();
        let challenge = self
            .oauth_client
            .spawn_session_authcode_listener(move || {
                cmd.unbounded_send(Command::CompleteSessionLogin).unwrap();
            })
            .await
            .map_err(|_| SpotifyError::LoginFailed)?;
        let auth_url = challenge.auth_url.clone();
        self.session_auth_challenge = Some(challenge);
        self.delegate.login_challenge_started(auth_url);
        Ok(())
    }

    fn finish_login(&mut self, new_session: Session) {
        let username = new_session.username();
        info!("Session created successfully for user: {username}");

        self.install_session(new_session);
        self.reconnect_attempts = 0;
        self.next_reconnect_at = None;

        // Abort any refresh loop from a previous session so we never run more
        // than one at a time.
        if let Some(handle) = self.token_refresh_task.take() {
            handle.abort();
        }

        let oauth_client = Arc::clone(&self.oauth_client);
        let command_sender = self.command_sender.clone();
        let refresh_task = tokio::task::spawn(async move {
            // Scheduling loop: wait until the token is near expiry, refresh,
            // and repeat. The refresh itself long-polls transient failures
            // inside refresh_token_at_expiry(), so an error surfaces here only
            // when it is fatal. In that case there is nothing left to retry;
            // exit and let a subsequent login spawn a fresh loop.
            //
            // The librespot session is deliberately not touched here: it keeps
            // its already-authenticated connection and cannot be reconnected
            // in place anyway (librespot sessions are single-use). Instead,
            // nudge the command loop, which rebuilds session + player if the
            // session happens to have died in the meantime, using the token
            // that was just stored.
            loop {
                match oauth_client.refresh_token_at_expiry().await {
                    Ok(_) => {
                        if command_sender
                            .unbounded_send(Command::ReconnectSession)
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Token refresh loop stopping: {e}");
                        break;
                    }
                }
            }
        });
        self.token_refresh_task = Some(refresh_task);

        self.spawn_session_watchdog();
        self.delegate.token_login_successful(username);
    }

    /// Swap in a freshly connected session: apply session attributes, build (or
    /// re-point) the player around it, and shut down the previous session (if
    /// any). Shared between the initial login and session rebuilds after a
    /// connection loss.
    ///
    /// Returns `true` when a brand new player had to be built (there was no
    /// live player to reuse) and `false` when the existing, still-alive player
    /// was simply re-pointed at the new session without interrupting playback.
    /// Callers use this to decide whether the current track needs reloading.
    fn install_session(&mut self, new_session: Session) -> bool {
        // Disable librespot's built-in explicit content filtering. When the
        // account has filter_enabled=true, Spotify sets the session attribute
        // "filter-explicit-content" to "1". This causes the audio key server
        // to deny decryption keys for ALL tracks (not just explicit ones),
        // breaking playback entirely. We override it to "0" so librespot can
        // load any track, and enforce the explicit filter ourselves at the
        // playback-state level where we only skip tracks actually marked
        // explicit.
        new_session.set_user_attribute("filter-explicit-content", "0");

        if let Some(old_session) = self.session.take() {
            if !old_session.is_invalid() {
                old_session.shutdown();
            }
        }
        self.session.replace(new_session.clone());

        match self.player.as_ref() {
            Some(player) if !player.is_invalid() => {
                info!("Player still alive; swapping new session in without interruption");
                player.set_session(new_session);
                false
            }
            _ => {
                self.build_and_store_player(new_session);
                true
            }
        }
    }

    /// Handle a track that failed to load on a healthy session: count the
    /// failure and, past the threshold, stop playback (DRM dialog if nothing
    /// has played, otherwise a transient toast). Below it, skip the track.
    fn handle_unavailable_track(&mut self) {
        warn!("Track unavailable, skipping");
        let failures = self
            .consecutive_load_failures
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if failures >= CONSECUTIVE_UNAVAILABLE_STOP_THRESHOLD {
            warn!("{failures} tracks failed to load back-to-back; stopping playback");
            // Nothing has ever played: signature of PlayPlay DRM. Otherwise transient.
            if !self.has_played_since_login.load(Ordering::Relaxed) {
                self.delegate.report_error(SpotifyError::PlaybackDrmBlocked);
            } else {
                self.delegate
                    .report_error(SpotifyError::PlaybackTemporarilyUnavailable);
            }
            // Reset so the stop can fire again if failures continue.
            self.consecutive_load_failures.store(0, Ordering::Relaxed);
            self.delegate.stop_playback();
            return;
        }
        self.delegate.end_of_track_reached();
        // The account's explicit filter may have changed; re-query it.
        let _ = self
            .command_sender
            .unbounded_send(Command::RecheckExplicitFilter);
    }

    /// Whether the librespot session or player died and must be recreated.
    /// librespot invalidates the session on any connection loss (network
    /// blip, suspend/resume, keepalive timeout) and it can never be
    /// reconnected; the player's command thread can also exit on its own.
    fn session_needs_rebuild(&self) -> bool {
        let session_dead = self.session.as_ref().is_some_and(|s| s.is_invalid());
        // A live session with no player is a broken state: either the player's
        // command thread exited, or the dev "kill player" tool dropped it.
        // Treat the missing player as dead so the watchdog rebuilds it. In
        // normal operation install_session always pairs a session with a
        // player, so a None player alongside a live session never occurs there.
        let player_dead = match self.player.as_ref() {
            Some(p) => p.is_invalid(),
            None => self.session.is_some(),
        };
        session_dead || player_dead
    }

    /// Rebuild the session if it died, before executing a playback command.
    /// When logged out this is a no-op; the command then fails with
    /// PlayerNotReady through get_player as before. User-initiated commands
    /// bypass the reconnect backoff on purpose; an explicit action should
    /// always try immediately.
    async fn ensure_session_alive(&mut self) -> Result<(), SpotifyError> {
        if self.session.is_none() || !self.session_needs_rebuild() {
            return Ok(());
        }
        self.rebuild_session().await
    }

    /// Rebuild the librespot session in the background when it looks dead,
    /// without disrupting playback that is still healthy.
    ///
    /// Called by the session watchdog, the token refresh loop and
    /// TrackUnavailable. Does nothing if the user is logged out, if a backoff
    /// retry is already scheduled, or if the session does not actually need a
    /// rebuild. `LoggedOut` and `NotPremium` are propagated to the caller;
    /// other (transient) failures are swallowed so the caller can keep going
    /// while `rebuild_session` schedules its own retry.
    async fn try_background_reconnect(&mut self) -> Result<(), SpotifyError> {
        if self.session.is_none() {
            // logged out; nothing to reconnect.
            return Ok(());
        }
        if let Some(at) = self.next_reconnect_at {
            if Instant::now() < at {
                // A backoff retry is already scheduled; don't hammer the
                // access points with parallel attempts.
                return Ok(());
            }
        }
        if !self.session_needs_rebuild() {
            return Ok(());
        }
        match self.rebuild_session().await {
            Ok(()) => {
                self.delegate.refresh_successful();
                Ok(())
            }
            Err(e @ (SpotifyError::LoggedOut | SpotifyError::NotPremium)) => Err(e),
            Err(e) => {
                warn!("Background session reconnect failed (will retry): {e}");
                Ok(())
            }
        }
    }

    /// Create a brand new session after the previous one died. On transient
    /// failure, schedules a retry with exponential back off.
    ///
    /// The current track is reloaded only when playback was actually interrupted:
    /// either a load already failed on the dead session (track_needs_reload),
    /// or the player itself died and had to be recreated. A dead session alone
    /// does not stop audio (tracks stream from the CDN and buffered audio keeps
    /// playing), so reloading unconditionally would yank playback backwards to a
    /// stale position.
    async fn rebuild_session(&mut self) -> Result<(), SpotifyError> {
        info!("librespot session died; rebuilding session");
        self.connection_lost.store(true, Ordering::Relaxed);
        self.delegate.set_connection_lost(true);

        let new_session = match connect_session_from_cache(self.settings.ap_port).await {
            Ok(s) => s,
            Err(SpotifyError::LoggedOut) => {
                error!("Cannot rebuild session: no usable cached session credentials");
                return Err(SpotifyError::LoggedOut);
            }
            Err(e) => {
                error!("Session rebuild failed: {e:?}");
                self.schedule_reconnect_retry();
                return Err(e);
            }
        };

        info!("Session rebuilt successfully");
        let player_recreated = self.install_session(new_session);
        self.reconnect_attempts = 0;
        self.next_reconnect_at = None;
        self.connection_lost.store(false, Ordering::Relaxed);
        self.delegate.set_connection_lost(false);

        if self.track_needs_reload || player_recreated {
            self.track_needs_reload = false;
            // A reload takes priority over deferred advancement: the track was
            // interrupted mid-playback, not finished.
            self.advance_after_reconnect.store(false, Ordering::Relaxed);
            if let Some(track) = self.current_track.clone() {
                let position_ms = self.last_position_ms.load(Ordering::Relaxed);
                let resume = !self.is_paused;
                info!("Resuming track after reconnect at {position_ms}ms (playing: {resume})");
                self.get_player_mut()?.load(track, resume, position_ms);
            }
        } else if self.advance_after_reconnect.swap(false, Ordering::Relaxed) {
            // The previous track finished (EndOfTrack) while the session was
            // down. Now that we have a healthy session, advance to the next
            // track in the queue.
            info!("Advancing to next track after reconnect (track ended during outage)");
            self.current_track = None;
            self.delegate.end_of_track_reached();
        }

        Ok(())
    }

    /// Schedule the next reconnect attempt with a fast exponential backoff.
    /// We retry aggressively so playback recovers quickly, and only back off
    /// mildly (capped at a few seconds) so a long outage still doesn't hammer
    /// Spotify's access points in a tight loop.
    fn schedule_reconnect_retry(&mut self) {
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        // 250ms, 500ms, 1s, 2s, 4s, then capped at 5s.
        let exp = self.reconnect_attempts.saturating_sub(1).min(5);
        let delay = Duration::from_millis(250 * (1u64 << exp)).min(Duration::from_secs(5));
        self.next_reconnect_at = Some(Instant::now() + delay);
        warn!("Next session reconnect attempt in {}ms", delay.as_millis());

        let sender = self.command_sender.clone();
        tokio::task::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = sender.unbounded_send(Command::ReconnectSession);
        });
    }

    /// Spawn a periodic task that nudges the command loop to check session
    /// health, so playback recovers from a dead session without waiting for
    /// user input. Spawned once for the lifetime of the player service; the
    /// ReconnectSession handler is a cheap no-op while the session is healthy.
    fn spawn_session_watchdog(&mut self) {
        if self.watchdog_spawned {
            return;
        }
        self.watchdog_spawned = true;
        let sender = self.command_sender.clone();
        tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // The first tick fires immediately; skip it, we just logged in.
            interval.tick().await;
            loop {
                interval.tick().await;
                if sender.unbounded_send(Command::ReconnectSession).is_err() {
                    break;
                }
            }
        });
    }

    /// Build a fresh player around `session`, spawn its event-listener task,
    /// and store it as the current player. Shared by every path that needs to
    /// (re)create the player: initial login, session rebuilds, and settings
    /// reloads.
    fn build_and_store_player(&mut self, session: Session) {
        let new_player = self.create_player(session);
        tokio::task::spawn(player_setup_delegate(
            new_player.get_player_event_channel(),
            self.delegate.clone(),
            self.command_sender.clone(),
            Arc::clone(&self.last_position_ms),
            Arc::clone(&self.connection_lost),
            Arc::clone(&self.advance_after_reconnect),
            Arc::clone(&self.consecutive_load_failures),
            Arc::clone(&self.has_played_since_login),
        ));
        self.player.replace(new_player);
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
            // Periodic PositionChanged events keep the last known playback
            // position fresh, so that resuming after a session rebuild
            // continues where the user actually was (audio keeps playing
            // from the CDN buffer long after the session itself dies).
            position_update_interval: Some(std::time::Duration::from_secs(1)),
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

fn librespot_cache_root() -> std::path::PathBuf {
    glib::user_cache_dir().join("riff").join("librespot")
}

fn open_librespot_cache() -> Option<Cache> {
    let root = librespot_cache_root();
    Cache::new(
        Some(root.join("credentials")),
        Some(root.join("volume")),
        Some(root.join("audio")),
        None,
    )
    .map_err(|e| dbg!(e))
    .ok()
}

fn session_minted_by_path() -> std::path::PathBuf {
    librespot_cache_root().join("credentials").join("minted-by")
}

fn note_session_minted() {
    if let Err(e) = std::fs::write(session_minted_by_path(), SESSION_CLIENT_ID) {
        warn!("Cannot note which client id minted the session credentials: {e}");
    }
}

fn session_cache_usable() -> bool {
    let Some(cache) = open_librespot_cache() else {
        return false;
    };
    if cache.credentials().is_none() {
        return false;
    }
    minted_the_session_way(std::fs::read_to_string(session_minted_by_path()).ok().as_deref())
}

fn minted_the_session_way(noted: Option<&str>) -> bool {
    noted.is_some_and(|noted| noted.trim() == SESSION_CLIENT_ID)
}

async fn connect_session_from_cache(ap_port: Option<u16>) -> Result<Session, SpotifyError> {
    let credentials = open_librespot_cache()
        .and_then(|cache| cache.credentials())
        .ok_or(SpotifyError::LoggedOut)?;
    create_session(&credentials, ap_port, false).await
}

async fn connect_session_from_token(
    access_token: &str,
    ap_port: Option<u16>,
) -> Result<Session, SpotifyError> {
    let credentials = Credentials::with_access_token(access_token);
    create_session(&credentials, ap_port, true).await
}

async fn create_session_with_port(
    credentials: &Credentials,
    ap_port: Option<u16>,
    store_credentials: bool,
) -> Result<Session, SpotifyError> {
    let session_config = SessionConfig {
        ap_port,
        ..Default::default()
    };
    let cache = open_librespot_cache();
    debug!("Connecting librespot session (ap_port={:?})", ap_port);
    let session = Session::new(session_config, cache);
    match session.connect(credentials.clone(), store_credentials).await {
        Ok(_) => {
            info!("librespot session connected successfully");
            Ok(session)
        }
        Err(err) => {
            error!(
                "librespot session connect failed (ap_port={:?}): {}",
                ap_port, err
            );
            // Distinguish rejected credentials from connectivity problems:
            // only the latter should make the caller try other AP ports.
            // librespot maps auth rejections to PermissionDenied (login
            // failed) and Unauthenticated (bad/expired token)
            match err.kind {
                ErrorKind::PermissionDenied | ErrorKind::Unauthenticated => {
                    Err(SpotifyError::LoginFailed)
                }
                _ => Err(SpotifyError::TechnicalError),
            }
        }
    }
}

async fn create_session(
    credentials: &Credentials,
    ap_port: Option<u16>,
    store_credentials: bool,
) -> Result<Session, SpotifyError> {
    // Dev-tools: when simulating offline, block new librespot sessions too.
    // The API client already rejects HTTP requests, but without this guard
    // the session watchdog would happily reconnect to Spotify's access
    // points, undermining the simulation.
    #[cfg(debug_assertions)]
    if crate::api::is_simulate_offline() {
        warn!("Blocking librespot session creation (simulate offline is active)");
        return Err(SpotifyError::TechnicalError);
    }

    match ap_port {
        Some(_) => create_session_with_port(credentials, ap_port, store_credentials).await,
        None => {
            let mut ports_to_try = KNOWN_AP_PORTS.iter();
            loop {
                if let Some(next_port) = ports_to_try.next() {
                    let res =
                        create_session_with_port(credentials, *next_port, store_credentials).await;
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
    last_position_ms: Arc<AtomicU32>,
    connection_lost: Arc<AtomicBool>,
    advance_after_reconnect: Arc<AtomicBool>,
    consecutive_load_failures: Arc<AtomicU32>,
    has_played_since_login: Arc<AtomicBool>,
) {
    while let Some(event) = channel.recv().await {
        match event {
            PlayerEvent::EndOfTrack { .. } => {
                if connection_lost.load(Ordering::Relaxed) {
                    // The session is down (buffered audio just played out the
                    // current track). Don't advance to the next song: loading
                    // it would fail on the dead session anyway. Instead, mark
                    // that we should advance once the session is rebuilt.
                    warn!("Track ended while disconnected; will advance after reconnect");
                    advance_after_reconnect.store(true, Ordering::Relaxed);
                } else {
                    delegate.end_of_track_reached();
                }
            }
            PlayerEvent::Unavailable { track_id, .. } => {
                // Defer to the command loop: whether this means "Skip the
                // track" or "the session died, reconnect and retry" depends
                // on the current session's health, which only the command
                // loop knows (this task's session may already be stale).
                warn!("Track could not be loaded: {track_id:?}");
                let _ = command_sender.unbounded_send(Command::TrackUnavailable);
            }
            PlayerEvent::Playing { position_ms, .. } => {
                // Audio is flowing again, so the session is definitely alive.
                // Clear any lingering "connection lost" state, but only touch
                // the delegate (and its UI) when the flag actually changes.
                if connection_lost.swap(false, Ordering::Relaxed) {
                    delegate.set_connection_lost(false);
                }
                // A track played: not DRM-blocked. Clear the failure run and,
                // on the first play this login, persist the account as known-good.
                consecutive_load_failures.store(0, Ordering::Relaxed);
                if !has_played_since_login.swap(true, Ordering::Relaxed) {
                    let _ = command_sender.unbounded_send(Command::MarkPlaybackVerified);
                }
                last_position_ms.store(position_ms, Ordering::Relaxed);
                delegate.notify_playback_state(position_ms);
            }
            PlayerEvent::Paused { position_ms, .. } => {
                last_position_ms.store(position_ms, Ordering::Relaxed);
            }
            // Periodic position report during playback (enabled via
            // PlayerConfig::position_update_interval). Keeps the resume
            // position fresh so a rebuild after a long outage doesn't jump
            // the track back to a stale position.
            PlayerEvent::PositionChanged { position_ms, .. } => {
                last_position_ms.store(position_ms, Ordering::Relaxed);
            }
            PlayerEvent::Seeked { position_ms, .. } => {
                last_position_ms.store(position_ms, Ordering::Relaxed);
            }
            PlayerEvent::TimeToPreloadNextTrack { .. } => {
                debug!("Requesting next track to be preloaded...");
                delegate.preload_next_track();
            }
            _ => {}
        }
    }
    debug!("Player event channel closed (player was replaced or dropped)");
}

/// Maps a 0.0–1.0 volume slider value to the mixer's u16 volume.
///
/// The VolumeCtrl curve (configured in create_player) determines the dB mapping.
/// The curve's db_range parameter (derived from volume_min_db/volume_max_db settings)
/// controls how many dB of dynamic range the slider spans.
fn mixer_set_volume(mixer: &mut dyn Mixer, volume: f64) {
    mixer.set_volume((VolumeCtrl::MAX_VOLUME as f64 * volume) as u16);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_from_before_the_split_are_not_reused() {
        assert!(
            !minted_the_session_way(None),
            "a cache with no note predates the split"
        );
        assert!(
            !minted_the_session_way(Some("782ae96ea60f4cdf986a766049607005")),
            "our registered client id cannot mint the session's credentials"
        );
        assert!(
            minted_the_session_way(Some(SESSION_CLIENT_ID)),
            "Spotify's own client id is the one that works"
        );
        assert!(
            minted_the_session_way(Some(&format!("{SESSION_CLIENT_ID}\n"))),
            "a trailing newline is still the same client id"
        );
    }
}
