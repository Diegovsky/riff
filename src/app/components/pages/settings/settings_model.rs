use crate::app::state::{PlaybackAction, SettingsAction};
use crate::app::{ActionDispatcher, AppModel};
use crate::settings::RiffSettings;
use std::rc::Rc;

pub struct SettingsModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl SettingsModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    pub fn stop_player(&self) {
        self.dispatcher.dispatch(PlaybackAction::Stop.into());
    }

    pub fn set_settings(&self) {
        self.dispatcher
            .dispatch(SettingsAction::ChangeSettings.into());
    }

    pub fn settings(&self) -> RiffSettings {
        let state = self.app_model.get_state();
        state.settings.settings.clone()
    }

    /// Whether the logged-in Spotify account has locked its explicit content
    /// filter (e.g. via a family plan parental control). When locked, the user
    /// cannot disable explicit-track skipping in Riff.
    pub fn explicit_filter_locked(&self) -> bool {
        self.app_model.get_state().playback.explicit_filter_locked()
    }
}
