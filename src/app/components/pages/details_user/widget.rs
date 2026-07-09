// Widget for the user profile detail page.
// Shows a circular avatar and the user's public playlists as a card grid.

use gettextrs::gettext;
use std::cell::Cell;
use std::rc::Rc;

use super::UserDetailsModel;

use crate::app::components::{
    CardLayout, CardSize, Component, DetailsPageComponent, EventListener, HasHeaderBarModel,
    SortOrder,
};
use crate::app::{ActionDispatcher, AppEvent, Worker};

/// GTK widget for the user profile detail page.
pub struct UserDetails {
    component: DetailsPageComponent<UserDetailsModel>,
}

impl UserDetails {
    pub fn new(
        model: UserDetailsModel,
        worker: Worker,
        shared_layout: Rc<Cell<CardLayout>>,
        shared_size: Rc<Cell<CardSize>>,
        dispatcher: Rc<dyn ActionDispatcher>,
    ) -> Self {
        let model = Rc::new(model);

        let mut component =
            DetailsPageComponent::new(model.clone(), model.to_headerbar_model(), worker);
        component.create_embedded_card_list(
            Some(&gettext("Public Playlists")),
            "user_playlists",
            &[
                SortOrder::RecentlyAdded,
                SortOrder::Alphabetic,
                SortOrder::Creator,
            ],
            shared_layout,
            shared_size,
            dispatcher,
        );

        Self { component }
    }
}

impl Component for UserDetails {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.component.get_root_widget()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for UserDetails {
    fn on_event(&mut self, event: &AppEvent) {
        self.component.handle_event(event);
        self.broadcast_event(event);
    }
}
