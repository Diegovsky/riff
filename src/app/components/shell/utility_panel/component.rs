use gio::prelude::*;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::UtilityPanelWidget;
use crate::app::components::{
    AppHeaderBar, Component, EventListener, QueueBarWidget, QueueList, QueueListModel, WindowLayout,
};
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, AppModel, Dispatcher};
use crate::feature_flags::FeatureFlag;
use crate::settings::SETTINGS;

const TOGGLE_ACTION: &str = "utility-panel";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    Side,
    Sheet,
}

impl Placement {
    fn from_flag(settings: &gio::Settings) -> Self {
        if settings.boolean(FeatureFlag::QueueSidePanel.key()) {
            Self::Side
        } else {
            Self::Sheet
        }
    }
}

struct Host {
    widget: UtilityPanelWidget,
    split: libadwaita::OverlaySplitView,
    sheet: libadwaita::BottomSheet,
    queue_bar: QueueBarWidget,
    layout: Rc<WindowLayout>,
    app_header: AppHeaderBar,
    action: gio::SimpleAction,
    placement: Cell<Placement>,
    has_track: Cell<bool>,
}

impl Host {
    fn is_open(&self) -> bool {
        match self.placement.get() {
            Placement::Side => self.split.shows_sidebar(),
            Placement::Sheet => self.sheet.is_open(),
        }
    }

    fn set_open(&self, open: bool) {
        match self.placement.get() {
            Placement::Side => self.layout.set_utility_open(open),
            Placement::Sheet => self.sheet.set_open(open),
        }
    }

    fn sync_header(&self) {
        let side = self.placement.get() == Placement::Side;
        self.app_header
            .set_utility_panel_state(side && self.is_open(), side);
    }

    fn sync_bottom_bar(&self, placement: Placement) {
        let in_sheet = placement == Placement::Sheet;
        self.queue_bar.set_show_track(!in_sheet);
        self.sheet.set_can_open(in_sheet);
        let shown = in_sheet || self.has_track.get();
        self.sheet.set_bottom_bar(shown.then_some(&self.queue_bar));
    }

    fn set_has_track(&self, has_track: bool) {
        self.has_track.set(has_track);
        self.sync_bottom_bar(self.placement.get());
    }

    fn place(&self, placement: Placement) {
        let in_sheet = placement == Placement::Sheet;
        self.sync_bottom_bar(placement);
        if self.placement.get() == placement {
            return;
        }
        self.set_open(false);
        match self.placement.get() {
            Placement::Side => self.split.set_sidebar(None::<&gtk::Widget>),
            Placement::Sheet => self.sheet.set_sheet(None::<&gtk::Widget>),
        }
        self.placement.set(placement);
        match placement {
            Placement::Side => self.split.set_sidebar(Some(&self.widget)),
            Placement::Sheet => self.sheet.set_sheet(Some(&self.widget)),
        }
        self.widget.set_in_sheet(in_sheet);
        self.widget
            .resize_handle()
            .set_visible(placement == Placement::Side && !self.split.is_collapsed());
        self.sync_header();
    }
}

pub struct UtilityPanel {
    widget: UtilityPanelWidget,
    host: Rc<Host>,
    queue_list: QueueList,
    app_model: Rc<AppModel>,
    _settings: gio::Settings,
}

impl UtilityPanel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: &libadwaita::ApplicationWindow,
        split: libadwaita::OverlaySplitView,
        sheet: libadwaita::BottomSheet,
        queue_bar: QueueBarWidget,
        widget: UtilityPanelWidget,
        layout: Rc<WindowLayout>,
        app_header: AppHeaderBar,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Self {
        let queue_model = QueueListModel::new(app_model.clone(), dispatcher, true);
        let queue_list = QueueList::new(Rc::new(queue_model));
        widget.set_queue_list(queue_list.widget());
        queue_list.connect_scrolling();
        widget.set_queue_empty(!queue_list.model().has_queue());

        let host = Rc::new(Host {
            widget: widget.clone(),
            split,
            sheet,
            queue_bar,
            layout,
            app_header,
            action: gio::SimpleAction::new_stateful(TOGGLE_ACTION, None, &false.to_variant()),
            placement: Cell::new(Placement::Side),
            has_track: Cell::new(app_model.get_state().playback.current_song().is_some()),
        });
        let settings = gio::Settings::new(SETTINGS);
        host.place(Placement::from_flag(&settings));
        settings.connect_changed(
            Some(FeatureFlag::QueueSidePanel.key()),
            clone!(
                #[weak]
                host,
                move |settings, _| host.place(Placement::from_flag(settings))
            ),
        );

        let panel = Self {
            widget,
            host,
            queue_list,
            app_model,
            _settings: settings,
        };
        panel.wire_toggle(window);
        panel
    }

    fn wire_toggle(&self, window: &libadwaita::ApplicationWindow) {
        let action = self.host.action.clone();
        let host = Rc::downgrade(&self.host);
        action.connect_change_state(move |_, state| {
            if let (Some(host), Some(open)) = (host.upgrade(), state.and_then(|s| s.get())) {
                host.set_open(open);
            }
        });
        window.add_action(&action);

        let host = Rc::downgrade(&self.host);
        let open_changed = Rc::new(move || {
            let Some(host) = host.upgrade() else {
                return;
            };
            action.set_state(&host.is_open().to_variant());
            host.sync_header();
        });
        self.host.split.connect_show_sidebar_notify(clone!(
            #[strong]
            open_changed,
            move |_| open_changed()
        ));
        self.host.sheet.connect_open_notify(move |_| open_changed());

        // Again once idle: opening it scrolls to the focused row
        let widget = self.widget.downgrade();
        self.host.sheet.connect_open_notify(move |sheet| {
            let Some(widget) = widget.upgrade() else {
                return;
            };
            if sheet.is_open() {
                widget.scroll_to_top();
                glib::idle_add_local_once(move || widget.scroll_to_top());
            }
        });
    }
}

impl Component for UtilityPanel {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }
}

impl EventListener for UtilityPanel {
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
            self.host
                .set_has_track(self.app_model.get_state().playback.current_song().is_some());
        }
    }
}
