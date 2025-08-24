use crate::{
    app::state::{AppAction, AppEvent, UpdatableState},
    settings::RiffSettings,
};

#[derive(Clone, Debug)]
pub enum SettingsAction {
    ChangeSettings,
}

impl From<SettingsAction> for AppAction {
    fn from(settings_action: SettingsAction) -> Self {
        Self::SettingsAction(settings_action)
    }
}

#[derive(Clone, Debug)]
pub enum SettingsEvent {
    PlayerSettingsChanged,
}

impl From<SettingsEvent> for AppEvent {
    fn from(settings_event: SettingsEvent) -> Self {
        Self::SettingsEvent(settings_event)
    }
}

#[derive(Default)]
pub struct SettingsState {
    // Probably shouldn't be stored, the source of truth is GSettings anyway
    pub settings: RiffSettings,
}

impl UpdatableState for SettingsState {
    type Action = SettingsAction;
    type Event = AppEvent;

    fn update_with(&mut self, action: std::borrow::Cow<Self::Action>) -> Vec<Self::Event> {
        match action.into_owned() {
            SettingsAction::ChangeSettings => {
                let old_settings = &self.settings;
                let new_settings = RiffSettings::new_from_gsettings().unwrap_or_default();
                let player_settings_changed =
                    new_settings.player_settings != old_settings.player_settings;
                self.settings = new_settings;
                if player_settings_changed {
                    vec![SettingsEvent::PlayerSettingsChanged.into()]
                } else {
                    vec![]
                }
            }
        }
    }
}
