use crate::{
    app::{
        components::{CardLayout, CardSize, EventListener, SortOrder},
        models::RepeatMode,
        state::{PlaybackAction, PlaybackEvent},
        AppAction, AppEvent, BrowserEvent,
    },
    player::{AudioBackend, SpotifyPlayerSettings, VolumeCurveType},
};
use gio::prelude::SettingsExt;
use libadwaita::ColorScheme;
use librespot::playback::config::{AudioFormat, Bitrate, NormalisationMethod, NormalisationType};

const SETTINGS: &str = "dev.diegovsky.Riff";

/// Spotify user id recorded as verified-playable, or empty if none.
pub fn drm_verified_user() -> String {
    gio::Settings::new(SETTINGS)
        .string("drm-verified-user")
        .to_string()
}

/// Records `user_id` as verified-playable so it isn't reported as DRM-blocked later.
pub fn set_drm_verified_user(user_id: &str) {
    let _ = gio::Settings::new(SETTINGS).set_string("drm-verified-user", user_id);
}

/// Clears the verified-playable account so DRM detection runs again. Dev tools only.
#[cfg(debug_assertions)]
pub fn clear_drm_verified_user() {
    let _ = gio::Settings::new(SETTINGS).set_string("drm-verified-user", "");
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloseWindowBehavior {
    #[default]
    Ask,
    MinimizeToBackground,
    StopAndQuit,
}

impl CloseWindowBehavior {
    pub fn from_gsettings_enum(value: i32) -> Self {
        match value {
            1 => Self::MinimizeToBackground,
            2 => Self::StopAndQuit,
            _ => Self::Ask,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WindowGeometry {
    pub width: i32,
    pub height: i32,
    pub is_maximized: bool,
}

impl WindowGeometry {
    pub fn new_from_gsettings() -> Self {
        let settings = gio::Settings::new(SETTINGS);
        Self {
            width: settings.int("window-width"),
            height: settings.int("window-height"),
            is_maximized: settings.boolean("window-is-maximized"),
        }
    }

    pub fn save(&self) -> Option<()> {
        let settings = gio::Settings::new(SETTINGS);
        settings.delay();
        settings.set_int("window-width", self.width).ok()?;
        settings.set_int("window-height", self.height).ok()?;
        settings
            .set_boolean("window-is-maximized", self.is_maximized)
            .ok()?;
        settings.apply();
        Some(())
    }
}

// Player (librespot) settings
impl SpotifyPlayerSettings {
    fn new_from_gsettings(settings: &gio::Settings) -> Option<Self> {
        let bitrate = match settings.enum_("player-bitrate") {
            0 => Some(Bitrate::Bitrate96),
            1 => Some(Bitrate::Bitrate160),
            2 => Some(Bitrate::Bitrate320),
            _ => None,
        }?;
        let backend = match settings.enum_("audio-backend") {
            0 => Some(AudioBackend::PulseAudio),
            1 => Some(AudioBackend::Alsa(
                settings.string("alsa-device").as_str().to_string(),
            )),
            _ => None,
        }?;
        let gapless = settings.boolean("gapless-playback");

        let ap_port_val = settings.uint("ap-port");
        // Access points usually use port 80, 443 or 4070. Since gsettings
        // does not allow optional values, we use 0 to indicate that any
        // port is OK and we should pass None to librespot's ap-port.
        let ap_port = match ap_port_val {
            1..=65535 => Some(ap_port_val as u16),
            _ => None,
        };

        let volume = settings.double("volume");
        let shuffle = settings.boolean("shuffle");
        let skip_explicit = settings.boolean("skip-explicit");
        let repeat = match settings.string("repeat").as_str() {
            "song" => RepeatMode::Song,
            "playlist" => RepeatMode::Playlist,
            "none" | _ => RepeatMode::None,
        };

        // Volume curve
        let volume_curve = match settings.enum_("volume-curve") {
            0 => VolumeCurveType::Log,
            1 => VolumeCurveType::Linear,
            2 => VolumeCurveType::Cubic,
            _ => VolumeCurveType::Log,
        };

        // Normalization
        let normalisation = settings.boolean("normalisation");
        let normalisation_type = match settings.enum_("normalisation-type") {
            0 => NormalisationType::Auto,
            1 => NormalisationType::Track,
            2 => NormalisationType::Album,
            _ => NormalisationType::Auto,
        };
        let normalisation_method = match settings.enum_("normalisation-method") {
            0 => NormalisationMethod::Dynamic,
            1 => NormalisationMethod::Basic,
            _ => NormalisationMethod::Dynamic,
        };
        let normalisation_pregain_db = settings.double("normalisation-pregain-db");
        let normalisation_threshold_dbfs = settings.double("normalisation-threshold-dbfs");
        let normalisation_attack_ms = settings.double("normalisation-attack-ms");
        let normalisation_release_ms = settings.double("normalisation-release-ms");
        let normalisation_knee_db = settings.double("normalisation-knee-db");

        // Audio format
        let audio_format = match settings.enum_("audio-format") {
            0 => AudioFormat::S16,
            1 => AudioFormat::S24,
            2 => AudioFormat::S24_3,
            3 => AudioFormat::S32,
            4 => AudioFormat::F32,
            5 => AudioFormat::F64,
            _ => AudioFormat::S16,
        };

        // Equalizer (active whenever any band is non-zero)
        let eq_bands = [
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

        // Mono audio
        let mono_audio = settings.boolean("mono-audio");

        // Stereo pan / balance (always enabled; centered has no effect)
        let pan = settings.double("pan");

        // Pitch shift in cents (0.0 = no shift)
        let pitch_cents = settings.double("pitch-cents");

        Some(Self {
            volume,
            repeat,
            shuffle,

            skip_explicit,

            bitrate,
            backend,
            gapless,
            ap_port,

            volume_curve,

            normalisation,
            normalisation_type,
            normalisation_method,
            normalisation_pregain_db,
            normalisation_threshold_dbfs,
            normalisation_attack_ms,
            normalisation_release_ms,
            normalisation_knee_db,

            audio_format,

            mono_audio,

            pan,

            pitch_cents,

            eq_bands,
        })
    }
    pub fn actions(&self) -> Vec<AppAction> {
        use PlaybackAction::*;
        vec![
            SetVolume(self.volume).into(),
            SetShuffled(self.shuffle).into(),
            SetRepeatMode(self.repeat).into(),
            SetSkipExplicit(self.skip_explicit).into(),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct RiffSettings {
    pub theme_preference: ColorScheme,
    pub player_settings: SpotifyPlayerSettings,
    pub window: WindowGeometry,
}

// Application settings
impl RiffSettings {
    pub fn new_from_gsettings() -> Option<Self> {
        let settings = gio::Settings::new(SETTINGS);
        let theme_preference = match settings.enum_("theme-preference") {
            0 => Some(ColorScheme::ForceLight),
            1 => Some(ColorScheme::ForceDark),
            2 => Some(ColorScheme::Default),
            _ => None,
        }?;
        Some(Self {
            theme_preference,
            player_settings: SpotifyPlayerSettings::new_from_gsettings(&settings)?,
            window: WindowGeometry::new_from_gsettings(),
        })
    }
}

impl Default for RiffSettings {
    fn default() -> Self {
        Self {
            theme_preference: ColorScheme::PreferDark,
            player_settings: Default::default(),
            window: Default::default(),
        }
    }
}

/// Observes some app state changes and records them into GSettings.
pub struct StateTracker {
    settings: gio::Settings,
}

type GResult = Result<(), glib::error::BoolError>;
impl StateTracker {
    pub fn new_from_gsettings() -> Self {
        Self {
            settings: gio::Settings::new(SETTINGS),
        }
    }
    fn on_playback_event(&self, event: &PlaybackEvent) -> GResult {
        use PlaybackEvent::*;
        match event {
            VolumeSet(volume) => self.settings.set_double("volume", *volume)?,
            ShuffleChanged(shuffle) => self.settings.set_boolean("shuffle", *shuffle)?,
            RepeatModeChanged(repeat) => self.settings.set_string(
                "repeat",
                match *repeat {
                    RepeatMode::Song => "song",
                    RepeatMode::Playlist => "playlist",
                    RepeatMode::None => "none",
                },
            )?,
            _ => (),
        }
        Ok(())
    }

    fn handle_event(&self, event: &AppEvent) -> GResult {
        match event {
            AppEvent::PlaybackEvent(event) => self.on_playback_event(event)?,
            AppEvent::BrowserEvent(BrowserEvent::CardLayoutChanged(layout)) => {
                self.save_card_layout(*layout);
            }
            AppEvent::BrowserEvent(BrowserEvent::CardSizeChanged(size)) => {
                self.save_card_size(*size);
            }
            AppEvent::BrowserEvent(BrowserEvent::SortOrderChanged(page, order)) => {
                self.save_sort_order(page, *order);
            }
            _ => (),
        }
        Ok(())
    }

    pub fn save_card_layout(&self, layout: CardLayout) {
        let _ = self.settings.set_string(
            "card-layout",
            match layout {
                CardLayout::Vertical => "vertical",
                CardLayout::ImageOnly => "image-only",
                CardLayout::Horizontal => "horizontal",
            },
        );
    }

    pub fn save_card_size(&self, size: CardSize) {
        let _ = self.settings.set_string(
            "card-size",
            match size {
                CardSize::Small => "small",
                CardSize::Medium => "medium",
                CardSize::Large => "large",
            },
        );
    }

    pub fn load_card_layout(&self) -> CardLayout {
        match self.settings.string("card-layout").as_str() {
            "image-only" => CardLayout::ImageOnly,
            "horizontal" => CardLayout::Horizontal,
            _ => CardLayout::Vertical,
        }
    }

    pub fn load_card_size(&self) -> CardSize {
        match self.settings.string("card-size").as_str() {
            "small" => CardSize::Small,
            "medium" => CardSize::Medium,
            _ => CardSize::Large,
        }
    }

    pub fn save_sort_order(&self, page: &str, order: SortOrder) {
        let key = format!("sort-{page}");
        if self
            .settings
            .settings_schema()
            .map_or(false, |s| s.has_key(&key))
        {
            let _ = self.settings.set_string(&key, order.to_str());
        }
    }

    pub fn load_sort_order(&self, page: &str) -> SortOrder {
        let key = format!("sort-{page}");
        if self
            .settings
            .settings_schema()
            .map_or(false, |s| s.has_key(&key))
        {
            SortOrder::parse_key(self.settings.string(&key).as_str())
        } else {
            SortOrder::RecentlyAdded
        }
    }
}

impl EventListener for StateTracker {
    fn on_event(&mut self, event: &AppEvent) {
        if let Err(e) = self.handle_event(event) {
            error!("Trying to update gsettings: {e}")
        }
    }
}
