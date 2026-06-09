// Widget for the artist detail page.
// Shows a circular artist photo, top tracks as a playlist, and album
// releases as a card grid.

use gettextrs::gettext;
use std::rc::Rc;

use super::ArtistDetailsModel;

use crate::app::components::{CardList, Component, DetailsPageComponent, EventListener, HasHeaderBarModel};
use crate::app::{AppEvent, Worker};
use crate::impl_details_component;

/// GTK widget for the artist detail page.
pub struct ArtistDetails {
    component: DetailsPageComponent<ArtistDetailsModel>,
    _card_list: CardList,
}

impl ArtistDetails {
    pub fn new(model: Rc<ArtistDetailsModel>, worker: Worker) -> Self {
        let mut component = DetailsPageComponent::new(
            model.clone(),
            model.to_headerbar_model(),
            worker,
        );
        component.create_playlist(Some(&gettext("Top tracks")));
        let card_list = component.create_card_list(Some(&gettext("Releases")));

        Self {
            component,
            _card_list: card_list,
        }
    }
}

impl_details_component!(ArtistDetails);
