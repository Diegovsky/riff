use crate::app::state::EntryId;
use crate::app::{AppModel, BrowserAction, Dispatcher};
use std::rc::Rc;

pub struct NavigationModel {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
}

impl NavigationModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    pub fn set_nav_hidden(&self, hidden: bool) {
        self.dispatcher
            .dispatch(BrowserAction::SetNavigationHidden(hidden).into());
    }

    pub fn go_back(&self) {
        self.dispatcher
            .dispatch(BrowserAction::NavigationBack.into());
    }

    pub fn go_forward(&self) {
        if self.app_model.get_state().browser.can_go_forward() {
            self.dispatcher
                .dispatch(BrowserAction::NavigationForward.into());
        }
    }

    pub fn save_scroll(&self, entry: EntryId, scroll: f64) {
        self.dispatcher
            .dispatch(BrowserAction::SaveScroll(entry, scroll.max(0.0) as u32).into());
    }
}
