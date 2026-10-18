use gio::prelude::SettingsExt;
use log::error;
use std::collections::HashMap;

pub const SETTINGS: &str = "dev.diegovsky.Riff";
pub const QUEUE_SIDE_PANEL: &str = "queue-side-panel";

/// Keys whose changes only apply after restarting the app.
pub const RESTART_REQUIRED_KEYS: &[&str] = &[
    // Audio
    "audio-backend",
    "alsa-device",
    "player-bitrate",
    "audio-format",
    "gapless-playback",
    "volume-curve",
    // Normalisation
    "normalisation",
    "normalisation-type",
    "normalisation-method",
    "normalisation-pregain-db",
    "normalisation-threshold-dbfs",
    "normalisation-attack-ms",
    "normalisation-release-ms",
    "normalisation-knee-db",
    // Network
    "ap-port",
    // Caching
    "audio-cache-size-mb",
];

/// GSettings key holding the per-user pinned-object map.
const PINNED_OBJECTS_KEY: &str = "pinned-objects-by-user";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinnedKind {
    Playlist,
    Album,
    Artist,
    Track,
}

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

pub fn prune_pinned_objects(user_id: &str, kind: PinnedKind, valid_ids: &[String]) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !prune_user_pins(&mut map, user_id, kind, valid_ids) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

pub fn drm_verified_user() -> String {
    gio::Settings::new(SETTINGS)
        .string("drm-verified-user")
        .to_string()
}

pub fn set_drm_verified_user(user_id: &str) {
    let _ = gio::Settings::new(SETTINGS).set_string("drm-verified-user", user_id);
}

#[cfg(debug_assertions)]
pub fn clear_drm_verified_user() {
    let _ = gio::Settings::new(SETTINGS).set_string("drm-verified-user", "");
}

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

pub fn card_layout() -> String {
    gio::Settings::new(SETTINGS)
        .string("card-layout")
        .to_string()
}

pub fn set_card_layout(layout: &str) {
    let _ = gio::Settings::new(SETTINGS).set_string("card-layout", layout);
}

pub fn card_size() -> String {
    gio::Settings::new(SETTINGS).string("card-size").to_string()
}

pub fn set_card_size(size: &str) {
    let _ = gio::Settings::new(SETTINGS).set_string("card-size", size);
}

fn sort_key(settings: &gio::Settings, page: &str) -> Option<String> {
    let key = format!("sort-{page}");
    settings
        .settings_schema()
        .is_some_and(|s| s.has_key(&key))
        .then_some(key)
}

pub fn sort_order(page: &str) -> Option<String> {
    let settings = gio::Settings::new(SETTINGS);
    let key = sort_key(&settings, page)?;
    Some(settings.string(&key).to_string())
}

pub fn set_sort_order(page: &str, order: &str) {
    let settings = gio::Settings::new(SETTINGS);
    if let Some(key) = sort_key(&settings, page) {
        let _ = settings.set_string(&key, order);
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
