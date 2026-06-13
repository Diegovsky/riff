// FlowBox-based card grid component with async image loading.
// Used for album grids, artist grids, and playlist grids.

use gtk::prelude::*;
use std::ops::Deref;

use crate::app::components::utils::wrap_flowbox_item;
use crate::app::components::{CardWidget, ImageShape};
use crate::app::dispatch::Worker;
use crate::app::models::CardModel;
use crate::app::ListStore;

/// Trait that abstracts the data/API layer for a card list.
/// Implement this for each page type (albums, playlists, artists, user playlists, etc.)
pub trait CardListModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_>;
    fn load_more(&self);
    fn refresh(&self);
    fn has_items(&self) -> bool;
    fn open_item(&self, id: String);
    fn image_shape(&self) -> ImageShape;
}

/// An embeddable FlowBox-based card grid. Does not own a ScrolledWindow.
/// Can be placed inside any container. Call `load_more()` externally when needed.
pub struct CardList {
    flowbox: gtk::FlowBox,
}

impl CardList {
    pub fn new() -> Self {
        let flowbox = gtk::FlowBox::new();
        flowbox.set_min_children_per_line(1);
        flowbox.set_max_children_per_line(12);
        flowbox.set_row_spacing(12);
        flowbox.set_column_spacing(12);
        flowbox.set_selection_mode(gtk::SelectionMode::None);
        flowbox.set_activate_on_single_click(true);
        Self { flowbox }
    }

    pub fn widget(&self) -> &gtk::FlowBox {
        &self.flowbox
    }

    pub fn bind<M: CardListModel + 'static>(&self, model: &std::rc::Rc<M>, worker: Worker) {
        if let Some(store) = model.get_store() {
            let shape = model.image_shape();
            let inner = store.inner().clone();
            let store_clone = inner.clone();

            self.flowbox.bind_model(Some(&inner), move |item| {
                wrap_flowbox_item(item, |card: &CardModel| {
                    CardWidget::for_model(card, worker.clone(), shape)
                })
            });

            let weak_model = std::rc::Rc::downgrade(model);
            self.flowbox.connect_child_activated(move |_, child| {
                let index = child.index() as u32;
                if let Some(item) = store_clone.item(index) {
                    if let Some(card) = item.downcast_ref::<CardModel>() {
                        if let Some(model) = weak_model.upgrade() {
                            model.open_item(card.id());
                        }
                    }
                }
            });
        }
    }
}
