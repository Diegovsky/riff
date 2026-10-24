use gio::prelude::SettingsExt;

use crate::settings::SETTINGS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeatureFlag {
    DeviceSelector,
    Normalisation,
    PinnedObjects,
}

impl FeatureFlag {
    pub const ALL: &[FeatureFlag] = &[
        FeatureFlag::DeviceSelector,
        FeatureFlag::Normalisation,
        FeatureFlag::PinnedObjects,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            FeatureFlag::DeviceSelector => "feature-device-selector",
            FeatureFlag::Normalisation => "feature-normalisation",
            FeatureFlag::PinnedObjects => "feature-pinned-objects",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            FeatureFlag::DeviceSelector => "Device Selector",
            FeatureFlag::Normalisation => "Audio Normalisation",
            FeatureFlag::PinnedObjects => "Pinned Items",
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
        }
    }
}

pub fn is_enabled(flag: FeatureFlag) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    settings.boolean(flag.key())
}
