use crate::app::state::ScreenName;
use crate::app::{AppModel, BrowserAction, Dispatcher};
use std::ops::Deref;
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

    pub fn visible_child_name(&self) -> impl Deref<Target = ScreenName> + '_ {
        self.app_model.map_state(|s| s.browser.current_screen())
    }

    pub fn set_nav_hidden(&self, hidden: bool) {
        self.dispatcher
            .dispatch(BrowserAction::SetNavigationHidden(hidden).into());
    }

    pub fn children_count(&self) -> usize {
        self.app_model.get_state().browser.count()
    }
}
