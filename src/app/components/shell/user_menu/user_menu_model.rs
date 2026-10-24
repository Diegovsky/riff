use crate::app::components::dispatch_api_read;
use crate::app::state::{LoginAction, PlaybackAction};
use crate::app::{AppModel, Dispatcher};
use std::ops::Deref;
use std::rc::Rc;

const USER_PLAYLISTS_PAGE: usize = 50;

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
        let Some(user) = self.username().map(|user| user.clone()) else {
            return;
        };
        dispatch_api_read(&self.dispatcher, move |tag| async move {
            let mut playlists = Vec::new();
            let mut offset = 0;
            loop {
                let page = api
                    .get_saved_playlists(offset, USER_PLAYLISTS_PAGE, tag)
                    .await?;
                offset += USER_PLAYLISTS_PAGE;
                let last = match page.total {
                    Some(total) => offset >= total,
                    None => page.items.len() < USER_PLAYLISTS_PAGE,
                };
                let empty = page.items.is_empty();
                playlists.extend(page.items);
                if last || empty {
                    break;
                }
            }
            let owned = playlists
                .iter()
                .filter(|p| p.owner.as_ref().is_some_and(|o| o.rri.id == user))
                .map(|p| p.rri.id.clone())
                .collect();
            let summaries = playlists.into_iter().map(|p| p.into()).collect();
            Ok(LoginAction::SetUserPlaylists(summaries, owned).into())
        });
    }
}
