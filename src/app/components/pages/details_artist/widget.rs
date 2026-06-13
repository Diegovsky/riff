// Widget for the artist detail page.
// Shows a circular artist photo, top tracks as a playlist, and album
// releases as a card grid.

use gettextrs::gettext;
use std::cell::Cell;
use std::rc::Rc;

use super::ArtistDetailsModel;

use crate::app::components::{CardLayout, CardSize, Component, DetailsPageComponent, EmbeddedCardList, EventListener, HasHeaderBarModel, SortOrder};
use crate::app::{ActionDispatcher, AppEvent, Worker};

/// GTK widget for the artist detail page.
pub struct ArtistDetails {
    component: DetailsPageComponent<ArtistDetailsModel>,
}

impl ArtistDetails {
    pub fn new(
        model: Rc<ArtistDetailsModel>,
        worker: Worker,
        shared_layout: Rc<Cell<CardLayout>>,
        shared_size: Rc<Cell<CardSize>>,
        dispatcher: Box<dyn ActionDispatcher>,
    ) -> Self {
        let mut component = DetailsPageComponent::new(
            model.clone(),
            model.to_headerbar_model(),
            worker,
        );
        component.create_playlist(Some(&gettext("Top tracks")));
        let card_list = component.create_card_list(Some(&gettext("Releases")));

        let embedded = EmbeddedCardList::new(
            card_list,
            "artist_releases",
            &[SortOrder::Alphabetic, SortOrder::DateReleased],
            shared_layout,
            shared_size,
            dispatcher,
        );

        if let Some(hb) = component.page().headerbar() {
            hb.pack_end(embedded.view_button());
        }

        component.add_child(Box::new(embedded));

        Self { component }
    }
}

impl Component for ArtistDetails {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.component.get_root_widget()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for ArtistDetails {
    fn on_event(&mut self, event: &AppEvent) {
        self.component.handle_event(event);
        self.broadcast_event(event);
    }
}
