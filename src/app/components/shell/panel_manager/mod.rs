use gio::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::app::components::{
    AppHeaderBar, BottomSheet, Component, EventListener, QueuePage, UtilityPanel,
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

struct Panels {
    queue: gtk::Widget,
    panel: UtilityPanel,
    sheet: BottomSheet,
    app_header: AppHeaderBar,
    action: gio::SimpleAction,
    placement: Cell<Option<Placement>>,
}

impl Panels {
    fn is_open(&self) -> bool {
        match self.placement.get() {
            Some(Placement::Side) => self.panel.is_open(),
            Some(Placement::Sheet) => self.sheet.is_open(),
            None => false,
        }
    }

    fn set_open(&self, open: bool) {
        match self.placement.get() {
            Some(Placement::Side) => self.panel.set_open(open),
            Some(Placement::Sheet) => self.sheet.set_open(open),
            None => {}
        }
    }

    fn sync_header(&self) {
        let side = self.placement.get() == Some(Placement::Side);
        self.app_header
            .set_utility_panel_state(side && self.is_open(), side);
    }

    fn place(&self, placement: Placement) {
        if self.placement.get() == Some(placement) {
            return;
        }
        self.set_open(false);
        match self.placement.get() {
            Some(Placement::Side) => self.panel.set_content(None, ""),
            Some(Placement::Sheet) => self.sheet.set_page(None),
            None => {}
        }
        self.placement.set(Some(placement));
        match placement {
            Placement::Side => self
                .panel
                .set_content(Some(&self.queue), &QueuePage::title()),
            Placement::Sheet => self.sheet.set_page(self.queue.downcast_ref()),
        }
        self.sync_header();
    }
}

pub struct PanelManager {
    queue: QueuePage,
    panels: Rc<Panels>,
    app_model: Rc<AppModel>,
    _settings: gio::Settings,
}

impl PanelManager {
    pub fn new(
        window: &libadwaita::ApplicationWindow,
        panel: UtilityPanel,
        sheet: BottomSheet,
        app_header: AppHeaderBar,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Self {
        let queue = QueuePage::new(app_model.clone(), dispatcher);
        let panels = Rc::new(Panels {
            queue: queue.widget().clone().upcast(),
            panel,
            sheet,
            app_header,
            action: gio::SimpleAction::new_stateful(TOGGLE_ACTION, None, &false.to_variant()),
            placement: Cell::new(None),
        });
        let settings = gio::Settings::new(SETTINGS);
        panels.place(Placement::from_flag(&settings));
        settings.connect_changed(
            Some(FeatureFlag::QueueSidePanel.key()),
            clone!(
                #[weak]
                panels,
                move |settings, _| panels.place(Placement::from_flag(settings))
            ),
        );

        let manager = Self {
            queue,
            panels,
            app_model,
            _settings: settings,
        };
        manager.wire_toggle(window);
        manager
    }

    fn wire_toggle(&self, window: &libadwaita::ApplicationWindow) {
        let action = self.panels.action.clone();
        let panels = Rc::downgrade(&self.panels);
        action.connect_change_state(move |_, state| {
            if let (Some(panels), Some(open)) = (panels.upgrade(), state.and_then(|s| s.get())) {
                panels.set_open(open);
            }
        });
        window.add_action(&action);

        let panels = Rc::downgrade(&self.panels);
        let open_changed = Rc::new(move || {
            let Some(panels) = panels.upgrade() else {
                return;
            };
            action.set_state(&panels.is_open().to_variant());
            panels.sync_header();
        });
        self.panels.panel.connect_open_notify(clone!(
            #[strong]
            open_changed,
            move || open_changed()
        ));
        self.panels
            .sheet
            .connect_open_notify(move || open_changed());
    }
}

impl Component for PanelManager {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.queue.get_root_widget()
    }
}

impl EventListener for PanelManager {
    fn on_event(&mut self, event: &AppEvent) {
        self.queue.on_event(event);
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::PlaybackStopped
            | PlaybackEvent::PlaylistChanged
            | PlaybackEvent::SourceChanged,
        ) = event
        {
            self.panels
                .sheet
                .set_has_track(self.app_model.get_state().playback.current_song().is_some());
        }
    }
}
