use gtk::prelude::*;
use std::rc::Rc;

use super::QueueListModel;
use crate::app::components::{Component, EventListener, TrackList};
use crate::app::AppEvent;

/// The play queue as a track list.
pub struct QueueList {
    model: Rc<QueueListModel>,
    listview: gtk::ListView,
    track_list: TrackList<QueueListModel>,
}

impl QueueList {
    pub fn new(model: Rc<QueueListModel>) -> Self {
        let listview = gtk::ListView::new(None::<gtk::NoSelection>, None::<gtk::ListItemFactory>);
        listview.set_margin_bottom(16);
        listview.add_css_class("playlist--grouped");

        let track_list = TrackList::new(listview.clone(), model.clone());
        Self {
            model,
            listview,
            track_list,
        }
    }

    pub fn widget(&self) -> &gtk::ListView {
        &self.listview
    }

    pub fn connect_scrolling(&self) {
        self.track_list.connect_scrolling();
    }

    pub fn model(&self) -> &Rc<QueueListModel> {
        &self.model
    }
}

impl Component for QueueList {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listview.upcast_ref()
    }
}

impl EventListener for QueueList {
    fn on_event(&mut self, event: &AppEvent) {
        self.track_list.on_event(event);
    }
}
