use gettextrs::gettext;
use gtk::prelude::*;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::{
    add_tracks_to_playlist, dispatch_api_call_many, Component, EventListener,
};
use crate::app::models::{PlaylistSummary, Track};
use crate::app::state::{
    LoginEvent, SelectionAction, SelectionContext, SelectionEvent, SelectionState,
};
use crate::app::{AppAction, AppEvent, AppModel, BrowserAction, Dispatcher};

use super::widget::{SelectionToolState, SelectionToolbarWidget};

pub struct SelectionToolbarModel {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
}

impl SelectionToolbarModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    pub fn queue_selection(&self) {
        self.dispatcher.dispatch(AppAction::QueueSelection);
    }

    fn dequeue_selection(&self) {
        self.dispatcher.dispatch(AppAction::DequeueSelection);
    }

    pub fn remove_selection(&self) {
        match &self.selection().context {
            SelectionContext::SavedTracks => self.remove_saved_tracks(),
            SelectionContext::Queue => self.dequeue_selection(),
            SelectionContext::EditablePlaylist(id) => self.remove_from_playlist(id),
            _ => {}
        }
    }

    pub fn save_selection(&self) {
        let api = self.app_model.api();
        let ids: Vec<String> = self
            .selection()
            .peek_selection()
            .map(|s| s.rri.id.clone())
            .collect();
        dispatch_api_call_many(&self.dispatcher, move || async move {
            api.save_tracks(ids).await?;
            Ok(vec![
                AppAction::SaveSelection,
                AppAction::ShowNotification(gettext("Tracks saved!")),
            ])
        })
    }

    fn remove_saved_tracks(&self) {
        let api = self.app_model.api();
        let ids: Vec<String> = self
            .selection()
            .peek_selection()
            .map(|s| s.rri.id.clone())
            .collect();
        dispatch_api_call_many(&self.dispatcher, move || async move {
            api.remove_tracks(ids).await?;
            Ok(vec![AppAction::UnsaveSelection])
        })
    }

    fn selection(&self) -> impl Deref<Target = SelectionState> + '_ {
        self.app_model.map_state(|s| &s.selection)
    }

    fn selected_count(&self) -> usize {
        self.selection().count()
    }

    fn user_playlists(&self) -> Vec<PlaylistSummary> {
        self.app_model.get_state().logged_user.owned_playlists()
    }

    fn add_to_playlist(&self, id: &str) {
        let tracks: Vec<Track> = self.selection().peek_selection().cloned().collect();
        add_tracks_to_playlist(
            &self.app_model,
            &self.dispatcher,
            id.to_string(),
            tracks,
            Some(SelectionAction::Clear.into()),
        );
    }

    fn remove_from_playlist(&self, id: &str) {
        let api = self.app_model.api();
        let id = id.to_string();
        let uris: Vec<String> = self
            .selection()
            .peek_selection()
            .filter_map(|s| s.rri.uri.clone())
            .collect();
        dispatch_api_call_many(&self.dispatcher, move || async move {
            api.remove_from_playlist(&id, uris.clone()).await?;
            Ok(vec![
                BrowserAction::RemoveTracksFromPlaylist(id, uris).into(),
                SelectionAction::Clear.into(),
            ])
        })
    }
}

pub struct SelectionToolbar {
    model: Rc<SelectionToolbarModel>,
    widget: SelectionToolbarWidget,
}

impl SelectionToolbar {
    pub fn new(model: SelectionToolbarModel, widget: SelectionToolbarWidget) -> Self {
        let model = Rc::new(model);
        widget.connect_queue(clone!(
            #[weak]
            model,
            move || model.queue_selection()
        ));
        widget.connect_remove(clone!(
            #[weak]
            model,
            move || model.remove_selection()
        ));
        widget.connect_save(clone!(
            #[weak]
            model,
            move || model.save_selection()
        ));
        Self { model, widget }
    }

    fn update_active_tools(&self) {
        let shown = SelectionToolState::Visible(self.model.selected_count() > 0);
        let hidden = SelectionToolState::Hidden;
        let (queue, add, remove, remove_from_playlist, save) = match self.model.selection().context
        {
            SelectionContext::Default => (shown, shown, hidden, hidden, shown),
            SelectionContext::SavedTracks => (shown, shown, shown, hidden, hidden),
            SelectionContext::ReadOnlyQueue => (hidden, hidden, hidden, hidden, shown),
            SelectionContext::Queue => (hidden, hidden, shown, hidden, shown),
            SelectionContext::Playlist => (shown, hidden, hidden, hidden, shown),
            SelectionContext::EditablePlaylist(_) => (shown, hidden, hidden, shown, shown),
        };
        self.widget.set_queue(queue);
        self.widget.set_add(add);
        self.widget.set_remove(remove);
        self.widget.set_remove_from_playlist(remove_from_playlist);
        self.widget.set_save(save);
    }
}

impl Component for SelectionToolbar {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }
}

impl EventListener for SelectionToolbar {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::SelectionEvent(SelectionEvent::SelectionModeChanged(active)) => {
                self.widget.set_visible(*active);
                self.update_active_tools();
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionChanged) => {
                self.update_active_tools();
            }
            AppEvent::LoginEvent(LoginEvent::UserPlaylistsLoaded) => {
                let model = &self.model;
                self.widget.connect_playlists(
                    &model.user_playlists(),
                    clone!(
                        #[weak]
                        model,
                        move |s| model.add_to_playlist(s)
                    ),
                );
            }
            _ => {}
        }
    }
}
