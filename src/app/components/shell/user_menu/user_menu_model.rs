use crate::app::components::dispatch_api_read;
use crate::app::state::{LoginAction, PlaybackAction};
use crate::app::{AppModel, Dispatcher};
use std::ops::Deref;
use std::rc::Rc;

pub struct UserMenuModel {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
}

impl UserMenuModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    pub fn username(&self) -> Option<impl Deref<Target = String> + '_> {
        self.app_model
            .map_state_opt(|s| s.logged_user.user.as_ref())
    }

    pub fn logout(&self) {
        self.dispatcher.dispatch(PlaybackAction::Stop.into());
        let api = self.app_model.api();
        let dispatcher = self.dispatcher.clone();
        tokio::spawn(async move {
            api.clear_user_cache().await;
            dispatcher.dispatch(LoginAction::Logout.into());
        });
    }

    pub fn fetch_user_playlists(&self) {
        let api = self.app_model.api();
        if self.username().is_some() {
            dispatch_api_read(&self.dispatcher, move |tag| async move {
                api.get_saved_playlists(0, 30, tag).await.map(|page| {
                    let summaries = page.items.into_iter().map(|p| p.into()).collect();
                    LoginAction::SetUserPlaylists(summaries).into()
                })
            });
        }
    }
}
