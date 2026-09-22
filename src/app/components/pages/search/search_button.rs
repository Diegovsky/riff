use gtk::prelude::*;

use crate::app::components::EventListener;
use crate::app::{AppAction, Dispatcher};

pub struct SearchBarModel(pub Dispatcher);

impl SearchBarModel {
    pub fn navigate_to_search(&self) {
        self.0.dispatch(AppAction::ViewSearch());
    }
}

pub struct SearchButton;

impl SearchButton {
    pub fn new(model: SearchBarModel, search_button: gtk::Button) -> Self {
        search_button.connect_clicked(move |_| {
            model.navigate_to_search();
        });

        Self
    }
}

impl EventListener for SearchButton {}
