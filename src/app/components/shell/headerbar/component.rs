use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glib::clone;

use crate::app::components::EventListener;
use crate::app::state::{SelectionAction, SelectionEvent};
use crate::app::{AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Dispatcher};

use super::widget::AppHeaderBar;

/// Selection behavior the shared header needs from the active screen.
/// Back and title are handled by the header itself.
pub trait HeaderBarModel {
    fn select_all(&self);
    fn selectable_keys(&self) -> Vec<String>;
    fn deselect_all(&self);
    fn cancel_selection(&self);
}

/// Per-screen selection hooks, wrapped into a [`HeaderBarModel`] by
/// [`SimpleHeaderBarModelWrapper`].
pub trait SimpleHeaderBarModel {
    fn select_all(&self);
    fn selectable_keys(&self) -> Vec<String> {
        vec![]
    }
}

pub struct SimpleHeaderBarModelWrapper<M> {
    wrapped_model: Rc<M>,
    dispatcher: Dispatcher,
}

impl<M> SimpleHeaderBarModelWrapper<M> {
    pub fn new(wrapped_model: Rc<M>, dispatcher: Dispatcher) -> Self {
        Self {
            wrapped_model,
            dispatcher,
        }
    }
}

impl<M> HeaderBarModel for SimpleHeaderBarModelWrapper<M>
where
    M: SimpleHeaderBarModel + 'static,
{
    fn select_all(&self) {
        self.wrapped_model.select_all()
    }

    fn selectable_keys(&self) -> Vec<String> {
        self.wrapped_model.selectable_keys()
    }

    fn deselect_all(&self) {
        self.dispatcher
            .dispatch(SelectionAction::DeselectAll.into())
    }

    fn cancel_selection(&self) {
        self.dispatcher.dispatch(AppAction::CancelSelection)
    }
}

fn all_selected(model: &dyn HeaderBarModel, app_model: &AppModel) -> bool {
    let state = app_model.get_state();
    if state.selection.count() == 0 {
        return false;
    }
    let keys = model.selectable_keys();
    !keys.is_empty() && keys.iter().all(|key| state.selection.is_song_selected(key))
}

/// Maps a header page name (see [`resolve_active_page`]) to the
/// [`HeaderBarModel`] for that screen. Screens populate it as they are built;
/// [`AppHeaderBarComponent`] reads it to rebind the header on screen changes.
pub type HeaderModelRegistry = Rc<RefCell<HashMap<String, Rc<dyn HeaderBarModel>>>>;

/// Home sub-page shown before the first [`BrowserEvent::HomeVisiblePageChanged`].
const DEFAULT_HOME_PAGE: &str = "library";

/// Resolve which header page is active. The "home" screen hosts several
/// sub-pages, so it follows the visible home sub-page; any other screen maps
/// to its own identifier.
fn resolve_active_page<'a>(current_screen: &'a str, home_page: &'a str) -> &'a str {
    if current_screen == "home" {
        home_page
    } else {
        current_screen
    }
}

/// Owns the app-wide header bar, swapping its title/button pages and
/// selection/back behavior when the active screen changes.
pub struct AppHeaderBarComponent {
    widget: AppHeaderBar,
    app_model: Rc<AppModel>,
    models: HeaderModelRegistry,
    active_model: Rc<RefCell<Option<Rc<dyn HeaderBarModel>>>>,
    home_page: String,
}

impl AppHeaderBarComponent {
    pub fn new(
        widget: AppHeaderBar,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        models: HeaderModelRegistry,
    ) -> Self {
        let active_model: Rc<RefCell<Option<Rc<dyn HeaderBarModel>>>> = Rc::new(RefCell::new(None));

        // Back navigates the browser stack.
        widget.connect_go_back(move || dispatcher.dispatch(BrowserAction::NavigationPop.into()));

        // Selection controls route to the active screen.
        widget.connect_select_all(clone!(
            #[strong]
            active_model,
            #[weak]
            app_model,
            move || {
                if let Some(model) = active_model.borrow().as_ref() {
                    if all_selected(&**model, &app_model) {
                        model.deselect_all();
                    } else {
                        model.select_all();
                    }
                }
            }
        ));
        widget.connect_selection_cancel(clone!(
            #[strong]
            active_model,
            move || {
                if let Some(model) = active_model.borrow().as_ref() {
                    model.cancel_selection();
                }
            }
        ));

        Self {
            widget,
            app_model,
            models,
            active_model,
            home_page: DEFAULT_HOME_PAGE.to_string(),
        }
    }

    fn current_screen_id(&self) -> String {
        self.app_model
            .get_state()
            .browser
            .current_screen()
            .identifier()
            .into_owned()
    }

    /// Recompute the active page and rebind the header's stacks and state.
    fn refresh_active(&self) {
        let current = self.current_screen_id();
        let active = resolve_active_page(&current, &self.home_page).to_string();

        self.widget.set_active(&active);

        let model = self.models.borrow().get(&active).cloned();
        self.widget.set_select_all_possible(model.is_some());
        *self.active_model.borrow_mut() = model;

        self.widget
            .set_can_go_back(self.app_model.get_state().browser.can_pop());
    }

    fn refresh_select_all(&self) {
        let all = self
            .active_model
            .borrow()
            .as_ref()
            .is_some_and(|model| all_selected(&**model, &self.app_model));
        self.widget.set_all_selected(all);
    }

    fn cancel_active_selection(&self) {
        let model = self.active_model.borrow().clone();
        if let Some(model) = model {
            model.cancel_selection();
        }
    }
}

impl EventListener for AppHeaderBarComponent {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Started => self.refresh_active(),
            AppEvent::BrowserEvent(
                BrowserEvent::NavigationPushed(_)
                | BrowserEvent::NavigationPopped
                | BrowserEvent::NavigationPoppedTo(_)
                | BrowserEvent::NavigationHidden(_),
            ) => {
                self.cancel_active_selection();
                self.refresh_active();
            }
            AppEvent::BrowserEvent(BrowserEvent::HomeVisiblePageChanged(page)) => {
                self.home_page = (*page).to_string();
                self.refresh_active();
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionModeChanged(active)) => {
                self.widget.set_selection_active(*active);
                self.refresh_select_all();
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionChanged) => {
                let count = self.app_model.get_state().selection.count();
                self.widget.set_selection_count(count);
                self.refresh_select_all();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_active_page;

    #[test]
    fn home_screen_follows_home_sub_page() {
        assert_eq!(resolve_active_page("home", "library"), "library");
        assert_eq!(resolve_active_page("home", "saved_tracks"), "saved_tracks");
        assert_eq!(resolve_active_page("home", "now_playing"), "now_playing");
    }

    #[test]
    fn pushed_screen_maps_to_itself() {
        assert_eq!(resolve_active_page("album_123", "library"), "album_123");
        assert_eq!(resolve_active_page("search", "saved_tracks"), "search");
        assert_eq!(
            resolve_active_page("artist_abc", "now_playing"),
            "artist_abc"
        );
    }
}
