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
use std::collections::HashMap;

pub const SETTINGS: &str = "dev.diegovsky.Riff";

/// GSettings key holding the per-user pinned-object map.
const PINNED_OBJECTS_KEY: &str = "pinned-objects-by-user";

/// The kind of media object a user can pin to the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinnedKind {
    Playlist,
    Album,
    Artist,
    Track,
}

/// A single user-pinned object. Serializes as `{"id":"...","kind":"...","title":"..."}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PinnedObject {
    pub id: String,
    pub kind: PinnedKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl PinnedObject {
    pub fn new(id: impl Into<String>, kind: PinnedKind) -> Self {
        Self {
            id: id.into(),
            kind,
            title: None,
        }
    }
}

/// Per-user map of pinned objects: `user_id -> Vec<PinnedObject>`.
type PinnedObjectsByUser = HashMap<String, Vec<PinnedObject>>;

fn load_pinned_map(settings: &gio::Settings) -> PinnedObjectsByUser {
    let json = settings.string(PINNED_OBJECTS_KEY);
    if json.is_empty() {
        return HashMap::new();
    }
    serde_json::from_str(&json).unwrap_or_else(|e| {
        error!("Failed to parse pinned objects: {e}");
        HashMap::new()
    })
}

fn save_pinned_map(settings: &gio::Settings, map: &PinnedObjectsByUser) -> bool {
    match serde_json::to_string(map) {
        Ok(json) => settings
            .set_string(PINNED_OBJECTS_KEY, &json)
            .map_err(|e| error!("Failed to save pinned objects: {e}"))
            .is_ok(),
        Err(e) => {
            error!("Failed to serialize pinned objects: {e}");
            false
        }
    }
}

fn pin_in_map(map: &mut PinnedObjectsByUser, user_id: &str, object: &PinnedObject) -> bool {
    let objects = map.entry(user_id.to_string()).or_default();
    if objects
        .iter()
        .any(|o| o.id == object.id && o.kind == object.kind)
    {
        return false;
    }
    objects.push(object.clone());
    true
}

fn unpin_from_map(
    map: &mut PinnedObjectsByUser,
    user_id: &str,
    id: &str,
    kind: PinnedKind,
) -> bool {
    let Some(objects) = map.get_mut(user_id) else {
        return false;
    };
    let len_before = objects.len();
    objects.retain(|o| o.id != id || o.kind != kind);
    let changed = objects.len() != len_before;
    if objects.is_empty() {
        map.remove(user_id);
    }
    changed
}

/// Drop pins for objects of `kind` whose IDs are no longer valid.
fn prune_user_pins(
    map: &mut PinnedObjectsByUser,
    user_id: &str,
    kind: PinnedKind,
    valid_ids: &[String],
) -> bool {
    let Some(objects) = map.get_mut(user_id) else {
        return false;
    };
    let len_before = objects.len();
    objects.retain(|o| o.kind != kind || valid_ids.iter().any(|valid| valid == &o.id));
    let changed = objects.len() != len_before;
    if objects.is_empty() {
        map.remove(user_id);
    }
    changed
}

// Generic pinned-object API (any kind).

pub fn get_pinned_objects(user_id: &str) -> Vec<PinnedObject> {
    load_pinned_map(&gio::Settings::new(SETTINGS))
        .get(user_id)
        .cloned()
        .unwrap_or_default()
}

pub fn is_object_pinned(user_id: &str, id: &str, kind: PinnedKind) -> bool {
    get_pinned_objects(user_id)
        .iter()
        .any(|o| o.id == id && o.kind == kind)
}

pub fn pin_object(user_id: &str, kind: PinnedKind, id: &str, title: Option<String>) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    let object = PinnedObject {
        title,
        ..PinnedObject::new(id, kind)
    };
    if !pin_in_map(&mut map, user_id, &object) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

pub fn unpin_object(user_id: &str, kind: PinnedKind, id: &str) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !unpin_from_map(&mut map, user_id, id, kind) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

/// Drop pins for objects of `kind` that are no longer in the user's library.
pub fn prune_pinned_objects(user_id: &str, kind: PinnedKind, valid_ids: &[String]) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !prune_user_pins(&mut map, user_id, kind, valid_ids) {
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

/// User-configured limit for librespot's on-disk audio cache, in bytes.
pub fn audio_cache_size_limit_bytes() -> u64 {
    let mb = gio::Settings::new(SETTINGS).uint("audio-cache-size-mb") as u64;
    mb * 1024 * 1024
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
            "song" => RepeatMode::Track,
            "playlist" => RepeatMode::Context,
            "none" | _ => RepeatMode::Off,
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
                    RepeatMode::Track => "song",
                    RepeatMode::Context => "playlist",
                    RepeatMode::Off => "none",
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

    fn pl(id: &str) -> PinnedObject {
        PinnedObject::new(id, PinnedKind::Playlist)
    }

    #[test]
    fn pin_adds_id() {
        let mut map = PinnedObjectsByUser::new();
        assert!(pin_in_map(&mut map, "user1", &pl("pl1")));
        assert_eq!(map["user1"], vec![pl("pl1")]);
    }

    #[test]
    fn double_pin_is_idempotent() {
        let mut map = PinnedObjectsByUser::new();
        assert!(pin_in_map(&mut map, "user1", &pl("pl1")));
        assert!(!pin_in_map(&mut map, "user1", &pl("pl1")));
        assert_eq!(map["user1"].len(), 1);
    }

    #[test]
    fn unpin_removes_id() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        assert!(unpin_from_map(
            &mut map,
            "user1",
            "pl1",
            PinnedKind::Playlist
        ));
        assert!(!map.contains_key("user1"));
    }

    #[test]
    fn unpin_last_item_yields_empty_map() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        unpin_from_map(&mut map, "user1", "pl1", PinnedKind::Playlist);
        assert!(map.is_empty());
    }

    #[test]
    fn is_playlist_pinned_checks_user_scope() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        assert!(map
            .get("user1")
            .unwrap()
            .iter()
            .any(|o| o.id == "pl1" && o.kind == PinnedKind::Playlist));
        assert!(!map
            .get("user2")
            .is_some_and(|objects| objects.iter().any(|o| o.id == "pl1")));
    }

    #[test]
    fn prune_drops_orphan_ids() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        pin_in_map(&mut map, "user1", &pl("pl2"));
        assert!(prune_user_pins(
            &mut map,
            "user1",
            PinnedKind::Playlist,
            &["pl1".to_string()],
        ));
        assert_eq!(map["user1"], vec![pl("pl1")]);
    }

    #[test]
    fn kind_is_part_of_identity() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("id1", PinnedKind::Album),
        );
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("id1", PinnedKind::Artist),
        );
        assert_eq!(map["user1"].len(), 2);
        assert!(unpin_from_map(&mut map, "user1", "id1", PinnedKind::Album));
        assert_eq!(map["user1"][0].kind, PinnedKind::Artist);
    }

    #[test]
    fn prune_is_kind_scoped() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("alb1", PinnedKind::Album),
        );
        assert!(prune_user_pins(
            &mut map,
            "user1",
            PinnedKind::Album,
            &["alb2".to_string()],
        ));
        assert_eq!(map["user1"].len(), 1);
        assert_eq!(map["user1"][0].kind, PinnedKind::Playlist);
    }

    #[test]
    fn modern_serialization_includes_kind() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("alb1", PinnedKind::Album),
        );
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains(r#""kind":"album""#));
        assert!(!json.contains("title"));
    }

    #[test]
    fn title_round_trips_and_is_optional() {
        let with_title: PinnedObject =
            serde_json::from_str(r#"{"id":"alb1","kind":"album","title":"Blue"}"#).unwrap();
        assert_eq!(with_title.title.as_deref(), Some("Blue"));
        let without_title: PinnedObject =
            serde_json::from_str(r#"{"id":"alb1","kind":"album"}"#).unwrap();
        assert_eq!(without_title.title, None);
    }
}
