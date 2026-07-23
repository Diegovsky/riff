use std::ops::Deref;
use std::rc::Rc;

use futures::channel::mpsc::UnboundedSender;
use gio::prelude::*;
use librespot::core::spotify_id::SpotifyId;
use librespot::core::SpotifyUri;

use crate::app::components::EventListener;
use crate::app::state::{
    Device, LoginAction, LoginEvent, LoginStartedEvent, PlaybackAction, PlaybackEvent,
    SettingsEvent,
};
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, SongsSource};
use crate::connect::ConnectCommand;
use crate::player::Command;

const SETTINGS: &str = "dev.diegovsky.Riff";

enum CurrentlyPlaying {
    WithSource {
        source: SongsSource,
        offset: usize,
        song: String,
    },
    Songs {
        songs: Vec<String>,
        offset: usize,
    },
}

impl CurrentlyPlaying {
    fn song_id(&self) -> &String {
        match self {
            Self::WithSource { song, .. } => song,
            Self::Songs { songs, offset } => &songs[*offset],
        }
    }
}

pub struct PlayerNotifier {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
    command_sender: UnboundedSender<Command>,
    connect_command_sender: UnboundedSender<ConnectCommand>,
    // Kept alive so its `changed` signals keep firing for live DSP updates.
    _dsp_settings: gio::Settings,
}

impl PlayerNotifier {
    pub fn new(
        app_model: Rc<AppModel>,
        dispatcher: Box<dyn ActionDispatcher>,
        command_sender: UnboundedSender<Command>,
        connect_command_sender: UnboundedSender<ConnectCommand>,
    ) -> Self {
        let dsp_settings = Self::watch_dsp_settings(command_sender.clone());
        Self::watch_skip_explicit_setting(&dsp_settings, dispatcher.as_ref());
        Self {
            app_model,
            dispatcher,
            command_sender,
            connect_command_sender,
            _dsp_settings: dsp_settings,
        }
    }

    /// Watch the skip-explicit GSettings key (the preferences toggle) and
    /// dispatch a PlaybackAction whenever it changes, so the playback state
    /// stays in sync with the user's local preference.
    fn watch_skip_explicit_setting(settings: &gio::Settings, dispatcher: &dyn ActionDispatcher) {
        let d = dispatcher.box_clone();
        settings.connect_changed(Some("skip-explicit"), move |settings, _| {
            let skip = settings.boolean("skip-explicit");
            d.dispatch(PlaybackAction::SetSkipExplicit(skip).into());
        });
    }

    /// Watch the equalizer, mono-audio, pan, and pitch GSettings keys and push
    /// changes to the local player immediately, so adjustments apply live with
    /// no player reload and no playback interruption.
    fn watch_dsp_settings(sender: UnboundedSender<Command>) -> gio::Settings {
        let settings = gio::Settings::new(SETTINGS);

        // Mono audio toggle.
        let s = sender.clone();
        settings.connect_changed(Some("mono-audio"), move |settings, _| {
            let enabled = settings.boolean("mono-audio");
            let _ = s.unbounded_send(Command::SetMono { enabled });
        });

        // Stereo pan / balance.
        let s = sender.clone();
        settings.connect_changed(Some("pan"), move |settings, _| {
            let pan = settings.double("pan");
            let _ = s.unbounded_send(Command::SetPan { pan });
        });

        // Pitch shift in cents.
        let s = sender.clone();
        settings.connect_changed(Some("pitch-cents"), move |settings, _| {
            let cents = settings.double("pitch-cents");
            let _ = s.unbounded_send(Command::SetPitch { cents });
        });

        // 10-band EQ: any band change sends the full band array.
        for band_key in &[
            "eq-band-0",
            "eq-band-1",
            "eq-band-2",
            "eq-band-3",
            "eq-band-4",
            "eq-band-5",
            "eq-band-6",
            "eq-band-7",
            "eq-band-8",
            "eq-band-9",
        ] {
            let s = sender.clone();
            settings.connect_changed(Some(band_key), move |settings, _| {
                let bands = [
                    settings.double("eq-band-0"),
                    settings.double("eq-band-1"),
                    settings.double("eq-band-2"),
                    settings.double("eq-band-3"),
                    settings.double("eq-band-4"),
                    settings.double("eq-band-5"),
                    settings.double("eq-band-6"),
                    settings.double("eq-band-7"),
                    settings.double("eq-band-8"),
                    settings.double("eq-band-9"),
                ];
                let _ = s.unbounded_send(Command::SetEqualizer { bands });
            });
        }

        settings
    }

    fn is_playing(&self) -> bool {
        self.app_model.get_state().playback.is_playing()
    }

    fn currently_playing(&self) -> Option<CurrentlyPlaying> {
        let state = self.app_model.get_state();
        let song = state.playback.current_song_id()?;
        let offset = state.playback.current_song_index()?;
        let source = state.playback.current_source().cloned();
        let result = match source {
            Some(source) if source.has_spotify_uri() => CurrentlyPlaying::WithSource {
                source,
                offset,
                song,
            },
            _ => CurrentlyPlaying::Songs {
                songs: state.playback.songs().map_collect(|s| s.id),
                offset,
            },
        };
        Some(result)
    }

    fn device(&self) -> impl Deref<Target = Device> + '_ {
        self.app_model.map_state(|s| s.playback.current_device())
    }

    fn notify_login(&self, event: &LoginEvent) {
        info!("notify_login: {:?}", event);
        let command = match event {
            LoginEvent::LoginStarted(LoginStartedEvent::Restore) => Some(Command::Restore),
            LoginEvent::LoginStarted(LoginStartedEvent::InitLogin) => Some(Command::InitLogin),
            LoginEvent::LoginStarted(LoginStartedEvent::CompleteLogin) => {
                Some(Command::CompleteLogin)
            }
            LoginEvent::FreshTokenRequested => Some(Command::RefreshToken),
            LoginEvent::LogoutCompleted => Some(Command::Logout),
            _ => None,
        };

        if let Some(command) = command {
            self.send_command_to_local_player(command);
        }
    }

    fn notify_connect_player(&self, event: &PlaybackEvent) {
        let event = event.clone();
        let currently_playing = self.currently_playing();
        let command = match event {
            PlaybackEvent::TrackChanged(_) | PlaybackEvent::SourceChanged => {
                match currently_playing {
                    Some(CurrentlyPlaying::WithSource {
                        source,
                        offset,
                        song,
                    }) => Some(ConnectCommand::PlayerLoadInContext {
                        source,
                        offset,
                        song,
                    }),
                    Some(CurrentlyPlaying::Songs { songs, offset }) => {
                        Some(ConnectCommand::PlayerLoad { songs, offset })
                    }
                    None => None,
                }
            }
            PlaybackEvent::TrackSeeked(position) => {
                Some(ConnectCommand::PlayerSeek(position as usize))
            }
            PlaybackEvent::PlaybackPaused => Some(ConnectCommand::PlayerPause),
            PlaybackEvent::PlaybackResumed => Some(ConnectCommand::PlayerResume),
            PlaybackEvent::VolumeSet(volume) => Some(ConnectCommand::PlayerSetVolume(
                (volume * 100f64).trunc() as u8,
            )),
            PlaybackEvent::RepeatModeChanged(mode) => Some(ConnectCommand::PlayerRepeat(mode)),
            PlaybackEvent::ShuffleChanged(shuffled) => {
                Some(ConnectCommand::PlayerShuffle(shuffled))
            }
            _ => None,
        };

        if let Some(command) = command {
            self.send_command_to_connect_player(command);
        }
    }

    fn notify_local_player(&self, event: &PlaybackEvent) {
        let command = match event {
            PlaybackEvent::PlaybackPaused => Some(Command::PlayerPause),
            PlaybackEvent::PlaybackResumed => Some(Command::PlayerResume),
            PlaybackEvent::PlaybackStopped => Some(Command::PlayerStop),
            PlaybackEvent::VolumeSet(volume) => Some(Command::PlayerSetVolume(*volume)),
            PlaybackEvent::SkipExplicitChanged(skip) => {
                // Sync GSettings with the internal state so the UI toggle
                // reflects changes forced by the account's explicit filter.
                // GSettings deduplicates writes of the same value, so this
                // won't cause an infinite loop with the watcher.
                self._dsp_settings.set_boolean("skip-explicit", *skip).ok();
                None
            }
            PlaybackEvent::TrackChanged(id) => {
                info!("track changed: {}", id);
                SpotifyId::from_base62(id)
                    .ok()
                    .map(|track| Command::PlayerLoad {
                        track: SpotifyUri::Track { id: track },
                        resume: true,
                    })
            }
            PlaybackEvent::SourceChanged => {
                let resume = self.is_playing();
                self.currently_playing()
                    .and_then(|c| SpotifyId::from_base62(c.song_id()).ok())
                    .map(|track| Command::PlayerLoad {
                        track: SpotifyUri::Track { id: track },
                        resume,
                    })
            }
            PlaybackEvent::TrackSeeked(position) => Some(Command::PlayerSeek(*position)),
            PlaybackEvent::Preload(id) => SpotifyId::from_base62(id)
                .ok()
                .map(|track| SpotifyUri::Track { id: track })
                .map(Command::PlayerPreload),
            _ => None,
        };

        if let Some(command) = command {
            self.send_command_to_local_player(command);
        }
    }

    fn send_command_to_connect_player(&self, command: ConnectCommand) {
        self.connect_command_sender.unbounded_send(command).unwrap();
    }

    fn send_command_to_local_player(&self, command: Command) {
        let dispatcher = &self.dispatcher;
        self.command_sender
            .unbounded_send(command)
            .unwrap_or_else(|_| {
                dispatcher.dispatch(AppAction::LoginAction(LoginAction::SetLoginFailure));
            });
    }

    fn switch_device(&mut self, device: &Device) {
        match device {
            Device::Connect(device) => {
                self.send_command_to_local_player(Command::PlayerStop);
                self.send_command_to_connect_player(ConnectCommand::SetDevice(device.id.clone()));
                self.notify_connect_player(&PlaybackEvent::SourceChanged);
            }
            Device::Local => {
                self.send_command_to_connect_player(ConnectCommand::PlayerStop);
                self.notify_local_player(&PlaybackEvent::SourceChanged);
            }
        }
    }
}

impl EventListener for PlayerNotifier {
    fn on_event(&mut self, event: &AppEvent) {
        let device = self.device().clone();
        match (device, event) {
            (_, AppEvent::LoginEvent(event)) => self.notify_login(event),
            (_, AppEvent::PlaybackEvent(PlaybackEvent::SwitchedDevice(d))) => self.switch_device(d),
            (Device::Local, AppEvent::PlaybackEvent(event)) => self.notify_local_player(event),
            (Device::Local, AppEvent::SettingsEvent(SettingsEvent::PlayerSettingsChanged)) => {
                self.send_command_to_local_player(Command::ReloadSettings)
            }
            (Device::Connect(_), AppEvent::PlaybackEvent(event)) => {
                self.notify_connect_player(event)
            }
            _ => {}
        }
    }
}
