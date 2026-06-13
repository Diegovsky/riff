// Widget for the playlist detail page.
// Shows playlist art, track list, and supports inline title editing
// when the user owns the playlist and selection mode is toggled.

use std::rc::Rc;

use super::PlaylistDetailsModel;

use crate::app::components::{Component, DetailsPageComponent, EventListener, HasHeaderBarModel};
use crate::app::dispatch::Worker;
use crate::app::state::SelectionEvent;
use crate::app::AppEvent;

/// GTK widget for the playlist detail page.
pub struct PlaylistDetails {
    model: Rc<PlaylistDetailsModel>,
    component: DetailsPageComponent<PlaylistDetailsModel>,
}

impl PlaylistDetails {
    pub fn new(model: Rc<PlaylistDetailsModel>, worker: Worker) -> Self {
        let mut component = DetailsPageComponent::new(
            model.clone(),
            model.to_headerbar_model(),
            worker,
        );
        component.create_playlist(None);

        Self { model, component }
    }

    fn set_editing(&self, editing: bool) {
        if !self.model.is_playlist_editable() {
            return;
        }
        if !editing {
            let new_name = self.component.page().header().get_title_text();
            let info = self.model.get_playlist_info();
            if let Some(info) = info {
                if new_name != info.title && !new_name.is_empty() {
                    self.model.update_playlist_details(new_name);
                } else {
                    self.component.page().header().set_title(&info.title);
                }
            }
        }
    }
}

impl Component for PlaylistDetails {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.component.get_root_widget()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for PlaylistDetails {
    fn on_event(&mut self, event: &AppEvent) {
        self.component.handle_event(event);
        if let AppEvent::SelectionEvent(SelectionEvent::SelectionModeChanged(active)) = event {
            self.set_editing(*active);
        }
        self.broadcast_event(event);
    }
}
