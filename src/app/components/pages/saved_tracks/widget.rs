// Widget for the "Liked Songs" (saved tracks) page.
// Wraps a DetailsPageComponent and triggers initial data load on login.

use std::rc::Rc;

use super::SavedTracksModel;
use crate::app::components::{
    Component, DetailsPageComponent, EventListener, HasHeaderBarModel, HeaderRegistrar,
};
use crate::app::state::LoginEvent;
use crate::app::{AppEvent, Worker};

/// GTK widget for the saved tracks (liked songs) detail page.
pub struct SavedTracks {
    model: Rc<SavedTracksModel>,
    component: DetailsPageComponent<SavedTracksModel>,
}

impl SavedTracks {
    pub fn new(
        model: Rc<SavedTracksModel>,
        worker: Worker,
        registrar: HeaderRegistrar,
        name: String,
    ) -> Self {
        let mut component = DetailsPageComponent::new(
            model.clone(),
            model.to_headerbar_model(),
            worker,
            registrar,
            name,
        );
        component.create_playlist(None);

        Self { model, component }
    }
}

impl Component for SavedTracks {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.component.get_root_widget()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for SavedTracks {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::LoginEvent(LoginEvent::LoginCompleted) = event {
            self.model.load_initial();
        }
        self.component.handle_event(event);
        self.broadcast_event(event);
    }
}
