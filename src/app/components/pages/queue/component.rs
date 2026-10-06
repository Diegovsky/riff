use gettextrs::gettext;
use gtk::prelude::*;
use std::rc::Rc;

use super::QueuePageWidget;
use crate::app::components::{Component, EventListener, QueueList, QueueListModel};
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, AppModel, Dispatcher};

pub struct QueuePage {
    widget: QueuePageWidget,
    queue_list: QueueList,
}

impl QueuePage {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        let queue_model = QueueListModel::new(app_model, dispatcher, true);
        let queue_list = QueueList::new(Rc::new(queue_model));
        let widget = QueuePageWidget::default();
        widget.set_queue_list(queue_list.widget());
        queue_list.connect_scrolling();
        widget.set_queue_empty(!queue_list.model().has_queue());
        Self { widget, queue_list }
    }

    pub fn widget(&self) -> &QueuePageWidget {
        &self.widget
    }

    pub fn title() -> String {
        // translators: Title of the queue, listing what plays next, when it's shown beside the content.
        gettext("Queue")
    }
}

impl Component for QueuePage {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }
}

impl EventListener for QueuePage {
    fn on_event(&mut self, event: &AppEvent) {
        self.queue_list.on_event(event);
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::PlaybackStopped
            | PlaybackEvent::PlaylistChanged
            | PlaybackEvent::SourceChanged,
        ) = event
        {
            self.widget
                .set_queue_empty(!self.queue_list.model().has_queue());
        }
    }
}
