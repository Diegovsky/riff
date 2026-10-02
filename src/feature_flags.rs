use gio::prelude::SettingsExt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeatureFlag {
    SelectMode,
    CreateNewPlaylist,
    DeviceSelector,
    Normalisation,
    PinnedObjects,
    QueueSidePanel,
}

impl FeatureFlag {
    pub const ALL: &[FeatureFlag] = &[
        FeatureFlag::SelectMode,
        FeatureFlag::CreateNewPlaylist,
        FeatureFlag::DeviceSelector,
        FeatureFlag::Normalisation,
        FeatureFlag::PinnedObjects,
        FeatureFlag::QueueSidePanel,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            FeatureFlag::SelectMode => "feature-select-mode",
            FeatureFlag::CreateNewPlaylist => "feature-create-new-playlist",
            FeatureFlag::DeviceSelector => "feature-device-selector",
            FeatureFlag::Normalisation => "feature-normalisation",
            FeatureFlag::PinnedObjects => "feature-pinned-objects",
            FeatureFlag::QueueSidePanel => "feature-queue-side-panel",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            FeatureFlag::SelectMode => "Select Mode",
            FeatureFlag::CreateNewPlaylist => "Create New Playlist",
            FeatureFlag::DeviceSelector => "Device Selector",
            FeatureFlag::Normalisation => "Audio Normalisation",
            FeatureFlag::PinnedObjects => "Pinned Items",
            FeatureFlag::QueueSidePanel => "Queue Side Panel",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            FeatureFlag::SelectMode => {
                "Enable selection mode to select multiple tracks for queuing, saving, or removing."
            }
            FeatureFlag::CreateNewPlaylist => "Enable the New Playlist button in the navigation panel.",
            FeatureFlag::DeviceSelector => {
                "Enable the device selector in the Now Playing headerbar."
            }
            FeatureFlag::Normalisation => {
                "Show audio normalisation settings for fine-tuning loudness between tracks."
            }
            FeatureFlag::PinnedObjects => {
                "Enable pinning saved playlists, albums, artists, and tracks to the navigation panel."
            }
            FeatureFlag::QueueSidePanel => {
                "Show the queue in a panel at the side of the window"
            }
        }
    }
}

pub fn is_enabled(flag: FeatureFlag) -> bool {
    let settings = gio::Settings::new(crate::settings::SETTINGS);
    settings.boolean(flag.key())
}
