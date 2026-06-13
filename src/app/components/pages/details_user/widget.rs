// Widget for the user profile detail page.
// Shows a circular avatar and the user's public playlists as a card grid.

use gettextrs::gettext;
use std::rc::Rc;

use super::UserDetailsModel;

use crate::app::components::{CardList, Component, DetailsPageComponent, EventListener, HasHeaderBarModel};
use crate::app::{AppEvent, Worker};
use crate::impl_details_component;

/// GTK widget for the user profile detail page.
pub struct UserDetails {
    component: DetailsPageComponent<UserDetailsModel>,
    _card_list: CardList,
}

impl UserDetails {
    pub fn new(model: UserDetailsModel, worker: Worker) -> Self {
        let model = Rc::new(model);

        let component = DetailsPageComponent::new(
            model.clone(),
            model.to_headerbar_model(),
            worker,
        );
        let card_list = component.create_card_list(Some(&gettext("Public Playlists")));

        Self {
            component,
            _card_list: card_list,
        }
    }
}

impl_details_component!(UserDetails);
