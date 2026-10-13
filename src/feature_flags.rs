use gio::prelude::SettingsExt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeatureFlag {
    DeviceSelector,
    Normalisation,
    PinnedObjects,
    QueueSidePanel,
}

impl FeatureFlag {
    pub const ALL: &[FeatureFlag] = &[
        FeatureFlag::DeviceSelector,
        FeatureFlag::Normalisation,
        FeatureFlag::PinnedObjects,
        FeatureFlag::QueueSidePanel,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            FeatureFlag::DeviceSelector => "feature-device-selector",
            FeatureFlag::Normalisation => "feature-normalisation",
            FeatureFlag::PinnedObjects => "feature-pinned-objects",
            FeatureFlag::QueueSidePanel => "feature-queue-side-panel",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            FeatureFlag::DeviceSelector => "Device Selector",
            FeatureFlag::Normalisation => "Audio Normalisation",
            FeatureFlag::PinnedObjects => "Pinned Items",
            FeatureFlag::QueueSidePanel => "Queue Side Panel",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
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
