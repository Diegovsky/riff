// Widget for the "Now Playing" page.
// The current track in the header, then the queue, or a placeholder when
// nothing plays.

use gettextrs::gettext;
use gtk::prelude::*;
use std::rc::Rc;

use super::NowPlayingModel;
use crate::app::components::{
    Component, DetailsPageComponent, DeviceSelector, DeviceSelectorWidget, EventListener,
    HasHeaderBarModel, HeaderRegistrar, QueueList,
};
use crate::app::state::PlaybackEvent;
use crate::app::AppEvent;
use crate::feature_flags::{self, FeatureFlag};

/// GTK widget for the now-playing/queue detail page.
pub struct NowPlaying {
    stack: gtk::Stack,
    component: DetailsPageComponent<NowPlayingModel>,
    #[allow(dead_code)]
    status_page: libadwaita::StatusPage,
}

impl NowPlaying {
    pub fn new(model: Rc<NowPlayingModel>, registrar: HeaderRegistrar, name: String) -> Self {
        let mut component =
            DetailsPageComponent::new(model.clone(), model.to_headerbar_model(), registrar, name);
        let queue_list = QueueList::new(model.queue_list_model());
        component.append_content(queue_list.widget());
        // Once the list is in the page's ScrolledWindow
        queue_list.connect_scrolling();
        component.add_child(Box::new(queue_list));

        if feature_flags::is_enabled(FeatureFlag::DeviceSelector) {
            let ds_widget: DeviceSelectorWidget = glib::Object::new();
            component.add_header_end(&ds_widget);
            let device_selector = Box::new(DeviceSelector::new(
                ds_widget,
                model.device_selector_model(),
            ));
            component.add_child(device_selector);
        }

        let status_page = libadwaita::StatusPage::new();
        status_page.set_title(&gettext("No Track Playing"));
        status_page.set_icon_name(Some("audio-x-generic-symbolic"));

        let stack = gtk::Stack::new();
        stack.add_named(component.get_root_widget(), Some("content"));
        stack.add_named(&status_page, Some("empty"));

        let visible = if model.has_queue() {
            "content"
        } else {
            "empty"
        };
        stack.set_visible_child_name(visible);

        Self {
            stack,
            component,
            status_page,
        }
    }

    fn update_empty_state(&self) {
        let name = if self.component.model().has_queue() {
            "content"
        } else {
            "empty"
        };
        self.stack.set_visible_child_name(name);
    }
}

impl Component for NowPlaying {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.stack.upcast_ref()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for NowPlaying {
    fn on_event(&mut self, event: &AppEvent) {
        self.component.handle_event(event);
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::PlaybackStopped
            | PlaybackEvent::PlaylistChanged,
        ) = event
        {
            self.update_empty_state();
        }
        self.broadcast_event(event);
    }
}
