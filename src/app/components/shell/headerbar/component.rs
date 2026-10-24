use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glib::clone;

use crate::app::components::navigation_panel::NavigationPanelDestination;
use crate::app::components::{labels, EventListener};
use crate::app::state::{Entry, HomePage, Route, SelectionAction, SelectionEvent};
use crate::app::{AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Dispatcher};

use super::widget::{AppHeaderBar, HistoryMenuItem, HistorySide};

/// Selection behavior the shared header needs from the active screen.
/// Back and title are handled by the header itself.
pub trait HeaderBarModel {
    fn select_all(&self);
    fn selectable_keys(&self) -> Vec<String>;
    fn deselect_all(&self);
    fn cancel_selection(&self);
    fn selects_queue(&self) -> bool;
}

/// Per-screen selection hooks, wrapped into a [`HeaderBarModel`] by
/// [`SimpleHeaderBarModelWrapper`].
pub trait SimpleHeaderBarModel {
    fn select_all(&self);
    fn selectable_keys(&self) -> Vec<String> {
        vec![]
    }
    fn selects_queue(&self) -> bool {
        false
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

    fn selects_queue(&self) -> bool {
        self.wrapped_model.selects_queue()
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

/// Most entries listed in a history menu.
const HISTORY_MENU_LENGTH: usize = 10;

fn resolve_active_page(route: &Route) -> String {
    match route {
        Route::Home(page) => page.id().to_string(),
        _ => route.screen_key(),
    }
}

fn history_menu<'a>(entries: impl Iterator<Item = &'a Entry>) -> Vec<HistoryMenuItem> {
    entries
        .take(HISTORY_MENU_LENGTH)
        .map(|entry| HistoryMenuItem {
            id: entry.id,
            icon: route_icon(&entry.route),
            label: entry
                .title
                .clone()
                .unwrap_or_else(|| labels::route_label(&entry.route)),
        })
        .collect()
}

fn route_icon(route: &Route) -> &'static str {
    match route {
        Route::Home(HomePage::NowPlaying) => NavigationPanelDestination::NowPlaying.icon(),
        Route::Home(_) => "paper-sheet-symbolic",
        Route::Album(_) => "library-music-symbolic",
        Route::Artist(_) | Route::User(_) => "avatar-default-symbolic",
        Route::Playlist(_) => "playlist2-symbolic",
        Route::Search => "system-search-symbolic",
    }
}

/// Owns the app-wide header bar, swapping its title/button pages and
/// selection/back behavior when the active screen changes.
pub struct AppHeaderBarComponent {
    widget: AppHeaderBar,
    app_model: Rc<AppModel>,
    models: HeaderModelRegistry,
    active_model: Rc<RefCell<Option<Rc<dyn HeaderBarModel>>>>,
}

impl AppHeaderBarComponent {
    pub fn new(
        widget: AppHeaderBar,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        models: HeaderModelRegistry,
    ) -> Self {
        let active_model: Rc<RefCell<Option<Rc<dyn HeaderBarModel>>>> = Rc::new(RefCell::new(None));

        widget.connect_go_back(clone!(
            #[strong]
            dispatcher,
            move || dispatcher.dispatch(BrowserAction::NavigationBack.into())
        ));
        widget.connect_go_forward(clone!(
            #[strong]
            dispatcher,
            move || dispatcher.dispatch(BrowserAction::NavigationForward.into())
        ));

        widget.connect_history_menu(clone!(
            #[weak]
            app_model,
            #[weak]
            widget,
            move |side| {
                let state = app_model.get_state();
                let navigator = state.browser.navigator();
                let items = match side {
                    HistorySide::Back => history_menu(navigator.back_entries()),
                    HistorySide::Forward => history_menu(navigator.forward_entries()),
                };
                drop(state);
                widget.show_history_menu(side, items);
            }
        ));
        widget.connect_history_pick(clone!(
            #[strong]
            dispatcher,
            move |id| dispatcher.dispatch(BrowserAction::NavigationGoTo(id).into())
        ));

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
        }
    }

    /// Recompute the active page and rebind the header's stacks and state.
    fn refresh_active(&self) {
        let active = resolve_active_page(self.app_model.get_state().browser.current_screen());

        self.widget.set_active(&active);

        let model = self.models.borrow().get(&active).cloned();
        self.widget.set_select_all_possible(model.is_some());
        *self.active_model.borrow_mut() = model;

        let state = self.app_model.get_state();
        self.widget.set_can_go_back(state.browser.can_go_back());
        self.widget
            .set_can_go_forward(state.browser.can_go_forward());
        drop(state);
        self.refresh_selection();
    }

    fn selecting_here(&self) -> bool {
        let queue = self
            .active_model
            .borrow()
            .as_ref()
            .is_some_and(|model| model.selects_queue());
        self.app_model.get_state().selection.is_selecting_in(queue)
    }

    fn refresh_selection(&self) {
        let active = self.selecting_here();
        if active != self.widget.is_selection_active() {
            self.widget.set_selection_active(active);
        }
        if active {
            let count = self.app_model.get_state().selection.count();
            self.widget.set_selection_count(count);
        }
        self.refresh_select_all();
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
        if !self.selecting_here() {
            return;
        }
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
                BrowserEvent::NavigationChanged(_) | BrowserEvent::NavigationHidden(_),
            ) => {
                self.cancel_active_selection();
                self.refresh_active();
            }
            AppEvent::SelectionEvent(
                SelectionEvent::SelectionModeChanged(_) | SelectionEvent::SelectionChanged,
            ) => self.refresh_selection(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_active_page;
    use crate::app::state::{HomePage, Route};

    #[test]
    fn home_screen_follows_home_sub_page() {
        assert_eq!(
            resolve_active_page(&Route::Home(HomePage::Library)),
            "library"
        );
        assert_eq!(
            resolve_active_page(&Route::Home(HomePage::SavedTracks)),
            "saved_tracks"
        );
        assert_eq!(
            resolve_active_page(&Route::Home(HomePage::NowPlaying)),
            "now_playing"
        );
    }

    #[test]
    fn other_screens_map_to_their_key() {
        assert_eq!(
            resolve_active_page(&Route::Album("123".into())),
            "album_123"
        );
        assert_eq!(resolve_active_page(&Route::Search), "search");
        assert_eq!(
            resolve_active_page(&Route::Artist("abc".into())),
            "artist_abc"
        );
    }
}
