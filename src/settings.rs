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
use serde_json;
use std::collections::HashMap;

pub const SETTINGS: &str = "dev.diegovsky.Riff";

const PINNED_PLAYLISTS_KEY: &str = "pinned-playlists-by-user";

type PinnedPlaylistsByUser = HashMap<String, Vec<String>>;

fn load_pinned_map(settings: &gio::Settings) -> PinnedPlaylistsByUser {
    let json = settings.string(PINNED_PLAYLISTS_KEY);
    if json.is_empty() {
        return HashMap::new();
    }
    serde_json::from_str(&json).unwrap_or_else(|e| {
        error!("Failed to parse pinned playlists: {e}");
        HashMap::new()
    })
}

fn save_pinned_map(settings: &gio::Settings, map: &PinnedPlaylistsByUser) -> bool {
    match serde_json::to_string(map) {
        Ok(json) => settings
            .set_string(PINNED_PLAYLISTS_KEY, &json)
            .map_err(|e| error!("Failed to save pinned playlists: {e}"))
            .is_ok(),
        Err(e) => {
            error!("Failed to serialize pinned playlists: {e}");
            false
        }
    }
}

fn pin_in_map(map: &mut PinnedPlaylistsByUser, user_id: &str, id: &str) -> bool {
    let ids = map.entry(user_id.to_string()).or_default();
    if ids.iter().any(|x| x == id) {
        return false;
    }
    ids.push(id.to_string());
    true
}

fn unpin_from_map(map: &mut PinnedPlaylistsByUser, user_id: &str, id: &str) -> bool {
    let Some(ids) = map.get_mut(user_id) else {
        return false;
    };
    let len_before = ids.len();
    ids.retain(|x| x != id);
    let changed = ids.len() != len_before;
    if ids.is_empty() {
        map.remove(user_id);
    }
    changed
}

fn prune_user_pins(map: &mut PinnedPlaylistsByUser, user_id: &str, valid_ids: &[String]) -> bool {
    let Some(ids) = map.get_mut(user_id) else {
        return false;
    };
    let len_before = ids.len();
    ids.retain(|id| valid_ids.iter().any(|valid| valid == id));
    let changed = ids.len() != len_before;
    if ids.is_empty() {
        map.remove(user_id);
    }
    changed
}

pub fn get_pinned_playlist_ids(user_id: &str) -> Vec<String> {
    load_pinned_map(&gio::Settings::new(SETTINGS))
        .get(user_id)
        .cloned()
        .unwrap_or_default()
}

pub fn is_playlist_pinned(user_id: &str, id: &str) -> bool {
    get_pinned_playlist_ids(user_id).iter().any(|x| x == id)
}

pub fn pin_playlist(user_id: &str, id: &str) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !pin_in_map(&mut map, user_id, id) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

pub fn unpin_playlist(user_id: &str, id: &str) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !unpin_from_map(&mut map, user_id, id) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

/// Drop pinned IDs that are no longer in the user's saved playlist set.
pub fn prune_pinned_playlists(user_id: &str, valid_ids: &[String]) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !prune_user_pins(&mut map, user_id, valid_ids) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

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
            error!("Trying to update gsettings: {e}");
        }
    }
}

#[cfg(test)]
mod pinned_playlists_tests {
    use super::*;

    #[test]
    fn pin_adds_id() {
        let mut map = PinnedPlaylistsByUser::new();
        assert!(pin_in_map(&mut map, "user1", "pl1"));
        assert_eq!(map["user1"], vec!["pl1".to_string()]);
    }

    #[test]
    fn double_pin_is_idempotent() {
        let mut map = PinnedPlaylistsByUser::new();
        assert!(pin_in_map(&mut map, "user1", "pl1"));
        assert!(!pin_in_map(&mut map, "user1", "pl1"));
        assert_eq!(map["user1"].len(), 1);
    }

    #[test]
    fn unpin_removes_id() {
        let mut map = PinnedPlaylistsByUser::new();
        pin_in_map(&mut map, "user1", "pl1");
        assert!(unpin_from_map(&mut map, "user1", "pl1"));
        assert!(!map.contains_key("user1"));
    }

    #[test]
    fn unpin_last_item_yields_empty_map() {
        let mut map = PinnedPlaylistsByUser::new();
        pin_in_map(&mut map, "user1", "pl1");
        unpin_from_map(&mut map, "user1", "pl1");
        assert!(map.is_empty());
    }

    #[test]
    fn is_playlist_pinned_checks_user_scope() {
        let mut map = PinnedPlaylistsByUser::new();
        pin_in_map(&mut map, "user1", "pl1");
        assert!(map.get("user1").unwrap().iter().any(|x| x == "pl1"));
        assert!(!map.get("user2").is_some_and(|ids| ids.iter().any(|x| x == "pl1")));
    }

    #[test]
    fn prune_drops_orphan_ids() {
        let mut map = PinnedPlaylistsByUser::new();
        pin_in_map(&mut map, "user1", "pl1");
        pin_in_map(&mut map, "user1", "pl2");
        assert!(prune_user_pins(
            &mut map,
            "user1",
            &["pl1".to_string()],
        ));
        assert_eq!(map["user1"], vec!["pl1".to_string()]);
    }
}
